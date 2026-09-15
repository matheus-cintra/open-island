//! QA-only executable. Uses the product transcriber and its linked native archives.
//! Input is the synthetic PCM16 WAV fixture, never a microphone.
use open_island_lib::voice::{pcm::Audio, transcribe};
use serde_json::json;
use std::{
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::Duration,
};

fn wave(path: &Path) -> Result<Audio, String> {
    let bytes = std::fs::read(path).map_err(|_| "fixture_unavailable")?;
    if bytes.len() > 2_000_000
        || bytes.get(..4) != Some(b"RIFF")
        || bytes.get(8..12) != Some(b"WAVE")
    {
        return Err("invalid_fixture".into());
    }
    let mut offset = 12;
    let mut format = false;
    let mut pcm = None;
    while offset + 8 <= bytes.len() {
        let length = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap()) as usize;
        let chunk = bytes
            .get(offset + 8..offset + 8 + length)
            .ok_or("invalid_fixture")?;
        match &bytes[offset..offset + 4] {
            b"fmt " => {
                format = chunk.len() >= 16
                    && chunk[..4] == [1, 0, 1, 0]
                    && chunk[4..8] == 16000u32.to_le_bytes()
                    && chunk[12..16] == [2, 0, 16, 0];
            }
            b"data" => pcm = Some(chunk),
            _ => {}
        }
        offset += 8 + length + length % 2;
    }
    let pcm = pcm
        .filter(|pcm| format && pcm.len().is_multiple_of(2))
        .ok_or("invalid_fixture")?;
    Ok(Audio {
        samples: pcm
            .as_chunks::<2>()
            .0
            .iter()
            .map(|sample| i16::from_le_bytes(*sample) as f32 / 32768.0)
            .collect(),
        sample_rate: 16000,
    })
}

fn main() {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() < 2 || args.len() > 4 {
        eprintln!("usage: voice_probe MODEL SYNTHETIC_WAV [--require-v1] [--cancel-after-ms=5000]");
        std::process::exit(2);
    }
    let mut require_v1 = false;
    let mut cancel_after_ms = None;
    for arg in &args[2..] {
        if arg == "--require-v1" && !require_v1 {
            require_v1 = true;
        } else if let Some(delay) = arg
            .to_str()
            .and_then(|arg| arg.strip_prefix("--cancel-after-ms="))
        {
            let delay = delay
                .parse::<u64>()
                .ok()
                .filter(|delay| (1..=60_000).contains(delay));
            if cancel_after_ms.is_some() || delay.is_none() {
                std::process::exit(2);
            }
            cancel_after_ms = delay;
        } else {
            std::process::exit(2);
        }
    }
    #[cfg(target_arch = "x86_64")]
    let cpu = {
        let mut cpu = serde_json::Map::new();
        // x86_64 guarantees CPUID. Decode the actual CPU feature bits;
        // this verifies the CPU exposed by QEMU, independently of build flags.
        let basic = std::arch::x86_64::__cpuid(1);
        let extended = std::arch::x86_64::__cpuid_count(7, 0);
        for (name, bits, shift) in [
            ("sse3", basic.ecx, 0),
            ("ssse3", basic.ecx, 9),
            ("sse4.1", basic.ecx, 19),
            ("sse4.2", basic.ecx, 20),
            ("avx", basic.ecx, 28),
            ("avx2", extended.ebx, 5),
            ("fma", basic.ecx, 12),
            ("f16c", basic.ecx, 29),
            ("bmi2", extended.ebx, 8),
        ] {
            cpu.insert(name.into(), json!((bits & (1 << shift)) != 0));
        }
        cpu
    };
    #[cfg(not(target_arch = "x86_64"))]
    let cpu = serde_json::Map::new();
    if require_v1 && (cpu.is_empty() || cpu.values().any(|value| value == true)) {
        eprintln!("CPU exceeds required v1 baseline: {}", json!(cpu));
        std::process::exit(1);
    }
    let start = std::time::Instant::now();
    let cancelled = AtomicBool::new(false);
    let (result, requested_at_ms) = std::thread::scope(|scope| {
        let (finished, receiver) = mpsc::channel::<()>();
        let timer = cancel_after_ms.map(|delay| {
            let cancelled = &cancelled;
            scope.spawn(move || {
                if matches!(
                    receiver.recv_timeout(Duration::from_millis(delay)),
                    Err(mpsc::RecvTimeoutError::Timeout)
                ) {
                    let requested_at = start.elapsed().as_millis();
                    cancelled.store(true, Ordering::Release);
                    Some(requested_at)
                } else {
                    None
                }
            })
        });
        let result = wave(Path::new(&args[1])).and_then(|audio| {
            transcribe::run(Path::new(&args[0]), audio, &cancelled).map_err(str::to_owned)
        });
        // Wake and join the timer even when loading fails before its deadline.
        drop(finished);
        let requested_at =
            timer.and_then(|timer| timer.join().expect("cancellation timer panicked"));
        (result, requested_at)
    });
    // The probe explicitly accepts synthetic fixtures: product code never logs text.
    println!(
        "{}",
        json!({"cpu":cpu,"elapsed_ms":start.elapsed().as_millis(),"result":result,
            "cancellation": {"after_ms":cancel_after_ms,"requested_at_ms":requested_at_ms}})
    );
}
