use cpal::{FromSample, Sample};
pub const MAX_SECONDS: usize = 60;
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Format,
    Overrun,
    DeviceLost,
    Cancelled,
    Unavailable,
    Permission,
}
impl Error {
    pub fn code(self) -> &'static str {
        match self {
            Self::Format => "audio_format_unsupported",
            Self::Overrun => "audio_overrun",
            Self::DeviceLost => "audio_device_lost",
            Self::Cancelled => "voice_cancelled",
            Self::Unavailable => "audio_unavailable",
            Self::Permission => "microphone_permission_denied",
        }
    }
}
pub struct Mono {
    samples: Vec<f32>,
    channels: usize,
    limit: usize,
    rate: u32,
}
impl Mono {
    pub fn new(rate: u32, channels: u16) -> Result<Self, Error> {
        if !(8000..=192000).contains(&rate) || !(1..=8).contains(&channels) {
            return Err(Error::Format);
        }
        let limit = rate as usize * MAX_SECONDS;
        if limit * size_of::<f32>() > MAX_BYTES {
            return Err(Error::Overrun);
        }
        let mut samples = Vec::new();
        samples
            .try_reserve_exact(limit)
            .map_err(|_| Error::Overrun)?;
        Ok(Self {
            samples,
            channels: channels as usize,
            limit,
            rate,
        })
    }
    pub fn push<T: Sample>(&mut self, input: &[T]) -> Result<bool, Error>
    where
        f32: FromSample<T>,
    {
        if !input.len().is_multiple_of(self.channels) {
            return Err(Error::Format);
        }
        let remaining = self.limit - self.samples.len();
        for frame in input.chunks_exact(self.channels).take(remaining) {
            let mut sum = 0.0;
            for sample in frame {
                let value = f32::from_sample(*sample);
                if !value.is_finite() {
                    return Err(Error::Format);
                }
                sum += value.clamp(-1.0, 1.0);
            }
            self.samples.push(sum / self.channels as f32);
        }
        Ok(self.samples.len() == self.limit)
    }
    pub fn len(&self) -> usize {
        self.samples.len()
    }
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }
    pub fn finish(self) -> Audio {
        Audio {
            samples: self.samples,
            sample_rate: self.rate,
        }
    }
}
pub struct Audio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn integer_unsigned_float_and_downmix_are_normalized() {
        let mut signed = Mono::new(48000, 2).unwrap();
        signed.push(&[i16::MIN, i16::MAX, 0, 0]).unwrap();
        assert!(signed.samples[0].abs() < 0.00002);
        assert_eq!(signed.samples[1], 0.0);
        let mut unsigned = Mono::new(44100, 1).unwrap();
        unsigned.push(&[0u8, 128, 255]).unwrap();
        assert_eq!(unsigned.samples, vec![-1.0, 0.0, 127.0 / 128.0]);
        let mut float = Mono::new(96000, 2).unwrap();
        float.push(&[0.5f64, -0.25, 2.0, -2.0]).unwrap();
        assert_eq!(float.samples, vec![0.125, 0.0]);
    }
    #[test]
    fn every_supported_pcm_storage_type_converts_to_mono_float() {
        fn check<T: Sample + FromSample<f32>>()
        where
            f32: FromSample<T>,
        {
            let input = [-0.75f32, 0.0, 0.5].map(T::from_sample);
            let mut buffer = Mono::new(8000, 1).unwrap();
            buffer.push(&input).unwrap();
            for (actual, expected) in buffer.samples.iter().zip([-0.75, 0.0, 0.5]) {
                assert!((actual - expected).abs() < 0.01);
            }
        }
        check::<i8>();
        check::<i16>();
        check::<cpal::I24>();
        check::<i32>();
        check::<i64>();
        check::<u8>();
        check::<u16>();
        check::<cpal::U24>();
        check::<u32>();
        check::<u64>();
        check::<f32>();
        check::<f64>();
    }
    #[test]
    fn duration_limit_never_reallocates_or_exceeds_memory_budget() {
        let mut buffer = Mono::new(192000, 8).unwrap();
        let capacity = buffer.samples.capacity();
        let address = buffer.samples.as_ptr();
        let block = [0i16; 8 * 1024];
        while !buffer.push(&block).unwrap() {}
        assert_eq!(buffer.samples.len(), 192000 * 60);
        assert!(buffer.samples.capacity() * size_of::<f32>() <= MAX_BYTES);
        assert!(buffer.push(&block).unwrap());
        assert_eq!(buffer.samples.capacity(), capacity);
        assert_eq!(buffer.samples.as_ptr(), address);
    }
    #[test]
    fn malformed_frames_nonfinite_samples_and_unsupported_config_fail() {
        assert!(Mono::new(192001, 1).is_err());
        assert!(Mono::new(48000, 9).is_err());
        let mut buffer = Mono::new(48000, 2).unwrap();
        assert_eq!(buffer.push(&[1f32]), Err(Error::Format));
        assert_eq!(buffer.push(&[f32::NAN, 0.0]), Err(Error::Format));
        assert!(buffer.samples.is_empty());
    }
}
