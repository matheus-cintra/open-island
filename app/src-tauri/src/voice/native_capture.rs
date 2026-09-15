use super::{
    capture::{Sink, Source},
    pcm::Error,
};
#[cfg(not(feature = "qa-harness"))]
use cpal::traits::HostTrait;
use cpal::{
    traits::{DeviceTrait, StreamTrait},
    SampleFormat,
};
use std::time::Duration;
fn audio_error(error: cpal::Error) -> Error {
    match error.kind() {
        cpal::ErrorKind::PermissionDenied => Error::Permission,
        cpal::ErrorKind::DeviceNotAvailable => Error::DeviceLost,
        cpal::ErrorKind::UnsupportedConfig | cpal::ErrorKind::UnsupportedOperation => Error::Format,
        _ => Error::Unavailable,
    }
}
pub trait PermissionGate {
    fn authorize(&self) -> Result<(), Error>;
}
pub struct Native {
    device: cpal::Device,
    config: cpal::SupportedStreamConfig,
}
impl Native {
    pub fn prepare(permission: &impl PermissionGate) -> Result<Self, Error> {
        #[cfg(feature = "qa-harness")]
        {
            let _ = permission;
            Err(Error::Unavailable)
        }
        #[cfg(not(feature = "qa-harness"))]
        {
            permission.authorize()?;
            let device = cpal::default_host()
                .default_input_device()
                .ok_or(Error::Unavailable)?;
            let config = device.default_input_config().map_err(audio_error)?;
            if !matches!(
                config.sample_format(),
                SampleFormat::I8
                    | SampleFormat::I16
                    | SampleFormat::I24
                    | SampleFormat::I32
                    | SampleFormat::I64
                    | SampleFormat::U8
                    | SampleFormat::U16
                    | SampleFormat::U24
                    | SampleFormat::U32
                    | SampleFormat::U64
                    | SampleFormat::F32
                    | SampleFormat::F64
            ) {
                return Err(Error::Format);
            }
            Ok(Self { device, config })
        }
    }
}
impl Source for Native {
    type Stream = cpal::Stream;
    fn format(&self) -> Result<(u32, u16), Error> {
        Ok((self.config.sample_rate(), self.config.channels()))
    }
    fn start(&self, sink: Sink) -> Result<cpal::Stream, Error> {
        let format = self.config.sample_format();
        let errors = sink.clone();
        let stream = self
            .device
            .build_input_stream_raw(
                self.config.config(),
                format,
                move |data, _| {
                    macro_rules! push {
                        ($ty:ty) => {
                            if let Some(samples) = data.as_slice::<$ty>() {
                                sink.push(samples);
                            } else {
                                sink.fail(Error::Format);
                            }
                        };
                    }
                    match format {
                        SampleFormat::I8 => push!(i8),
                        SampleFormat::I16 => push!(i16),
                        SampleFormat::I24 => push!(cpal::I24),
                        SampleFormat::I32 => push!(i32),
                        SampleFormat::I64 => push!(i64),
                        SampleFormat::U8 => push!(u8),
                        SampleFormat::U16 => push!(u16),
                        SampleFormat::U24 => push!(cpal::U24),
                        SampleFormat::U32 => push!(u32),
                        SampleFormat::U64 => push!(u64),
                        SampleFormat::F32 => push!(f32),
                        SampleFormat::F64 => push!(f64),
                        _ => sink.fail(Error::Format),
                    }
                },
                move |_| errors.fail(Error::DeviceLost),
                Some(Duration::from_secs(2)),
            )
            .map_err(audio_error)?;
        stream.play().map_err(audio_error)?;
        Ok(stream)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Denied(AtomicUsize);
    impl PermissionGate for Denied {
        fn authorize(&self) -> Result<(), Error> {
            self.0.fetch_add(1, Ordering::Relaxed);
            Err(Error::Permission)
        }
    }
    #[test]
    fn permission_is_required_and_qa_never_calls_the_native_permission_adapter() {
        let permission = Denied(AtomicUsize::new(0));
        let result = Native::prepare(&permission).err();
        if cfg!(feature = "qa-harness") {
            assert_eq!(result, Some(Error::Unavailable));
            assert_eq!(permission.0.load(Ordering::Relaxed), 0);
        } else {
            assert_eq!(result, Some(Error::Permission));
            assert_eq!(permission.0.load(Ordering::Relaxed), 1);
        }
    }
}
