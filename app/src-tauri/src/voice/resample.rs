use super::pcm::{Audio, Error, MAX_BYTES, MAX_SECONDS};
use audioadapter_buffers::direct::InterleavedSlice;
use rubato::{Fft, FixedSync, Resampler};
use std::sync::atomic::{AtomicBool, Ordering};

/// Runs only after the capture stream has been dropped. Never persists audio.
pub fn mono_16khz(mut audio: Audio, cancelled: &AtomicBool) -> Result<Vec<f32>, Error> {
    if cancelled.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    if !(8000..=192000).contains(&audio.sample_rate)
        || audio
            .samples
            .iter()
            .any(|sample| !sample.is_finite() || sample.abs() > 1.0)
    {
        return Err(Error::Format);
    }
    if audio.samples.len() > audio.sample_rate as usize * MAX_SECONDS
        || audio.samples.len() > MAX_BYTES / size_of::<f32>()
    {
        return Err(Error::Overrun);
    }
    if audio.samples.is_empty() || audio.sample_rate == 16000 {
        return Ok(audio.samples);
    }
    let original_len = audio.samples.len();
    let expected_len = (original_len * 16000).div_ceil(audio.sample_rate as usize);
    let mut divisor = audio.sample_rate as usize;
    let mut remainder = 16000;
    while remainder != 0 {
        (divisor, remainder) = (remainder, divisor % remainder);
    }
    let quantum = audio.sample_rate as usize / divisor;
    let chunk = quantum * 2 * 1024usize.div_ceil(quantum * 2);
    // rubato 4.0.0's first delay trim copies `delay` frames. FixedBoth with
    // even FFT sizes makes that exactly all valid first-block frames and
    // avoids a fractional-sample delay.
    let mut resampler =
        Fft::<f32>::new(audio.sample_rate as usize, 16000, chunk, 1, FixedSync::Both)
            .map_err(|_| Error::Format)?;
    // The delay trim is in process_all's full-block loop. Ensure short clips
    // enter it, then trim the zero padding back to the original duration.
    let padded_len = original_len.max(resampler.input_frames_next() + 1);
    audio.samples.resize(padded_len, 0.0);
    let input =
        InterleavedSlice::new(&audio.samples, 1, audio.samples.len()).map_err(|_| Error::Format)?;
    let mut output = resampler
        .process_all(&input, audio.samples.len(), None)
        .map_err(|_| Error::Format)?
        .take_data();
    output.truncate(expected_len);
    if cancelled.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_blocks_preserve_duration_and_signal_without_filter_delay() {
        for rate in [8000, 44100, 48000, 96000, 192000] {
            let length = rate as usize * 2 + 137;
            let samples = (0..length)
                .map(|index| {
                    (index as f32 * std::f32::consts::TAU * 440.0 / rate as f32).sin() * 0.5
                })
                .collect();
            let output = mono_16khz(
                Audio {
                    samples,
                    sample_rate: rate,
                },
                &AtomicBool::new(false),
            )
            .unwrap();
            let expected = (length as f64 * 16000.0 / rate as f64).round() as usize;
            assert!(output.len().abs_diff(expected) <= 1, "rate {rate}");
            let mse = output
                .iter()
                .enumerate()
                .skip(200)
                .take(output.len() - 400)
                .map(|(index, sample)| {
                    let expected =
                        (index as f32 * std::f32::consts::TAU * 440.0 / 16000.0).sin() * 0.5;
                    (sample - expected).powi(2)
                })
                .sum::<f32>()
                / (output.len() - 400) as f32;
            assert!(mse < 0.00001, "rate {rate}, mse {mse}");
        }
    }

    #[test]
    fn short_clips_keep_the_first_sample_instead_of_filter_startup_silence() {
        for rate in [8000, 44100, 48000, 96000, 192000] {
            for length in [1, 137, 1001] {
                let mut samples = vec![0.0; length];
                samples[0] = 1.0;
                let output = mono_16khz(
                    Audio {
                        samples,
                        sample_rate: rate,
                    },
                    &AtomicBool::new(false),
                )
                .unwrap();
                assert_eq!(output.len(), (length * 16000).div_ceil(rate as usize));
                assert!(output[0] > 0.01, "rate {rate}, length {length}");
                assert!(output.iter().all(|sample| sample.is_finite()));
                let peak = output
                    .iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| a.abs().total_cmp(&b.abs()))
                    .unwrap()
                    .0;
                assert_eq!(peak, 0, "rate {rate}, length {length}");
            }
        }
    }

    #[test]
    fn silence_empty_passthrough_and_invalid_audio_are_handled() {
        let cancel = AtomicBool::new(false);
        let silence = mono_16khz(
            Audio {
                samples: vec![0.0; 44100 * 3],
                sample_rate: 44100,
            },
            &cancel,
        )
        .unwrap();
        assert_eq!(silence.len(), 48000);
        assert!(silence.iter().all(|sample| *sample == 0.0));
        for samples in [vec![], vec![0.25; 77]] {
            assert_eq!(
                mono_16khz(
                    Audio {
                        samples: samples.clone(),
                        sample_rate: 16000
                    },
                    &cancel
                )
                .unwrap(),
                samples
            );
        }
        for (samples, sample_rate, error) in [
            (vec![0.0], 0, Error::Format),
            (vec![f32::NAN], 16000, Error::Format),
            (vec![1.1], 16000, Error::Format),
            (vec![0.0; 8000 * 60 + 1], 8000, Error::Overrun),
        ] {
            assert_eq!(
                mono_16khz(
                    Audio {
                        samples,
                        sample_rate
                    },
                    &cancel
                ),
                Err(error)
            );
        }
        cancel.store(true, Ordering::Release);
        assert_eq!(
            mono_16khz(
                Audio {
                    samples: vec![],
                    sample_rate: 16000
                },
                &cancel
            ),
            Err(Error::Cancelled)
        );
    }
}
