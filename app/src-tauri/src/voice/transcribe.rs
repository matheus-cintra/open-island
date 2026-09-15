use super::{model, pcm::Audio, resample};
use std::{
    ffi::c_void,
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};
use whisper_rs::{FullParams, SamplingStrategy, WhisperContext, WhisperContextParameters};

unsafe extern "C" fn should_abort(data: *mut c_void) -> bool {
    // The caller retains this AtomicBool throughout synchronous state.full().
    // This callback only reads the flag; it never accesses Whisper state.
    unsafe { &*data.cast::<AtomicBool>() }.load(Ordering::Acquire)
}

/// A single CPU inference. Context/state and PCM are dropped on every exit.
/// Call on the job's native worker, never on the render or audio callback.
pub fn run(path: &Path, audio: Audio, cancelled: &AtomicBool) -> Result<String, &'static str> {
    let samples = resample::mono_16khz(audio, cancelled).map_err(|error| error.code())?;
    if samples.is_empty() || samples.iter().all(|sample| sample.abs() <= 1e-7) {
        return Err("no_speech");
    }
    let path = model::validate(path)?;
    check_cancel(cancelled)?;
    // Neither logging feature is enabled: these once-installed hooks suppress
    // native model paths and inference payloads, including loader errors.
    whisper_rs::install_logging_hooks();
    let mut context_params = WhisperContextParameters::default();
    context_params.use_gpu(false);
    // Match whisper.cpp's context default explicitly: whisper-rs defaults this
    // CPU-capable attention implementation off. Decoding settings stay fixed.
    context_params.flash_attn(true);
    let context =
        WhisperContext::new_with_params(path, context_params).map_err(|_| "invalid_model")?;
    check_cancel(cancelled)?;
    if !context.is_multilingual() {
        return Err("model_language_unsupported");
    }
    let mut state = context.create_state().map_err(|_| "transcription_failed")?;
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 1 });
    params.set_n_threads(std::thread::available_parallelism().map_or(1, |n| n.get().min(4)) as i32);
    params.set_language(Some("pt"));
    params.set_detect_language(false);
    params.set_translate(false);
    params.set_no_context(true);
    params.set_no_timestamps(true);
    params.set_print_special(false);
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_timestamps(false);
    // Use the raw callback with borrowed lifetime instead of the 0.16.0 safe
    // setter, which boxes a trait object without retaining its ownership.
    // full() is synchronous and joins native work before this borrow can end.
    unsafe {
        params.set_abort_callback(Some(should_abort));
        params.set_abort_callback_user_data(std::ptr::from_ref(cancelled).cast_mut().cast());
    }
    let result = state.full(params, &samples);
    check_cancel(cancelled)?;
    result.map_err(|_| "transcription_failed")?;
    let mut text = String::new();
    for segment in state.as_iter() {
        check_cancel(cancelled)?;
        text.push_str(segment.to_str().map_err(|_| "transcription_failed")?);
    }
    let text = text.trim().to_owned();
    check_cancel(cancelled)?;
    if text.is_empty() {
        Err("no_speech")
    } else {
        Ok(text)
    }
}

fn check_cancel(cancelled: &AtomicBool) -> Result<(), &'static str> {
    if cancelled.load(Ordering::Acquire) {
        Err("voice_cancelled")
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn silence_cancel_and_invalid_audio_never_need_a_model() {
        let missing = Path::new("missing-voice-model.bin");
        let cancelled = AtomicBool::new(false);
        assert_eq!(
            run(
                missing,
                Audio {
                    samples: vec![0.0; 48000],
                    sample_rate: 16000
                },
                &cancelled
            ),
            Err("no_speech")
        );
        assert_eq!(
            run(
                missing,
                Audio {
                    samples: vec![0.1; 16000 * 60 + 1],
                    sample_rate: 16000
                },
                &cancelled
            ),
            Err("audio_overrun")
        );
        cancelled.store(true, Ordering::Release);
        assert_eq!(
            run(
                missing,
                Audio {
                    samples: vec![0.1; 16000],
                    sample_rate: 16000
                },
                &cancelled
            ),
            Err("voice_cancelled")
        );
    }
    #[test]
    fn abort_callback_reads_the_owned_job_flag() {
        let cancelled = AtomicBool::new(false);
        let pointer = std::ptr::from_ref(&cancelled).cast_mut().cast();
        assert!(!unsafe { should_abort(pointer) });
        cancelled.store(true, Ordering::Release);
        assert!(unsafe { should_abort(pointer) });
    }
}
