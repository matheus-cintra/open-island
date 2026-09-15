use serde::{Deserialize, Serialize};
use std::io::Read;

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct Audio {
    /// Hardware enumeration is not a microphone permission or capture test.
    pub alsa_capture_devices: Option<bool>,
    pub capture_tested: bool,
}

pub fn collect() -> Audio {
    #[cfg(all(target_os = "linux", not(feature = "qa-harness")))]
    {
        Audio {
            alsa_capture_devices: std::fs::File::open("/proc/asound/pcm")
                .ok()
                .and_then(|file| capture_devices(file).ok()),
            capture_tested: false,
        }
    }
    #[cfg(any(not(target_os = "linux"), feature = "qa-harness"))]
    Audio::default()
}

#[cfg_attr(
    any(not(target_os = "linux"), feature = "qa-harness"),
    allow(dead_code)
)]
fn capture_devices(reader: impl Read) -> std::io::Result<bool> {
    const LIMIT: usize = 32768;
    let mut bytes = Vec::new();
    reader.take((LIMIT + 1) as u64).read_to_end(&mut bytes)?;
    if bytes.len() > LIMIT {
        return Err(std::io::Error::other("audio_probe_limit"));
    }
    let text =
        std::str::from_utf8(&bytes).map_err(|_| std::io::Error::other("audio_probe_invalid"))?;
    // Device labels are discarded; only the kernel's final capability fields count.
    Ok(text.lines().any(|line| {
        line.split(" : ").skip(2).any(|field| {
            field
                .trim()
                .strip_prefix("capture ")
                .is_some_and(|count| count.parse::<u32>().is_ok_and(|count| count > 0))
        })
    }))
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_capabilities_exclude_names_playback_and_zero_capture() {
        for input in [
            "",
            "00-00: capture 1 : Secret device : playback 1\n",
            "00-00: Device : Device : capture 0\n",
        ] {
            assert!(!capture_devices(input.as_bytes()).unwrap());
        }
        assert!(capture_devices(
            b"00-00: Secret device : Private label : playback 1 : capture 1\n".as_slice()
        )
        .unwrap());
        let report = Audio {
            alsa_capture_devices: Some(true),
            capture_tested: false,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("Secret"));
        assert!(!report.capture_tested);
    }

    #[test]
    fn oversize_invalid_and_failed_reads_remain_unknown() {
        assert!(capture_devices(vec![b'x'; 32769].as_slice()).is_err());
        assert!(capture_devices([0xff].as_slice()).is_err());
        struct Failed;
        impl Read for Failed {
            fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
                Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
            }
        }
        assert!(capture_devices(Failed).is_err());
        assert!(Audio::default().alsa_capture_devices.is_none());
    }
}
