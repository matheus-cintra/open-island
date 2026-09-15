use super::pcm::{Audio, Error, Mono, MAX_SECONDS};
use cpal::{FromSample, Sample};
use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Clone, Default)]
pub struct Controls {
    pub samples: Arc<AtomicU64>,
    pub overruns: Arc<AtomicU64>,
    pub stop: Arc<AtomicBool>,
    pub cancel: Arc<AtomicBool>,
}
#[derive(Clone)]
pub struct Sink {
    buffer: Arc<Mutex<Option<Mono>>>,
    status: Arc<AtomicU8>,
    controls: Controls,
}
impl Sink {
    pub fn push<T: Sample>(&self, input: &[T])
    where
        f32: FromSample<T>,
    {
        if self.status.load(Ordering::Acquire) != 0
            || self.controls.cancel.load(Ordering::Acquire)
            || self.controls.stop.load(Ordering::Acquire)
        {
            return;
        }
        let Ok(mut guard) = self.buffer.try_lock() else {
            self.fail(Error::Overrun);
            return;
        };
        let Some(buffer) = guard.as_mut() else {
            return;
        };
        let result = buffer.push(input);
        self.controls
            .samples
            .store(buffer.len() as u64, Ordering::Release);
        match result {
            Ok(true) => {
                let _ = self
                    .status
                    .compare_exchange(0, 1, Ordering::AcqRel, Ordering::Acquire);
            }
            Ok(false) => {}
            Err(error) => self.fail(error),
        }
    }
    pub fn fail(&self, error: Error) {
        let status = match error {
            Error::Format => 2,
            Error::Overrun => 3,
            _ => 4,
        };
        if self
            .status
            .compare_exchange(0, status, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
            && error == Error::Overrun
        {
            self.controls.overruns.fetch_add(1, Ordering::Relaxed);
        }
    }
}
pub trait Source {
    type Stream;
    fn format(&self) -> Result<(u32, u16), Error>;
    fn start(&self, sink: Sink) -> Result<Self::Stream, Error>;
}
pub fn run(source: &impl Source, controls: Controls) -> Result<Audio, Error> {
    run_until(source, controls, Duration::from_secs(MAX_SECONDS as u64))
}
fn run_until(source: &impl Source, controls: Controls, limit: Duration) -> Result<Audio, Error> {
    if controls.cancel.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    let (rate, channels) = source.format()?;
    if controls.stop.load(Ordering::Acquire) {
        return Ok(Audio {
            samples: Vec::new(),
            sample_rate: rate,
        });
    }
    let sink = Sink {
        buffer: Arc::new(Mutex::new(Some(Mono::new(rate, channels)?))),
        status: Arc::new(AtomicU8::new(0)),
        controls: controls.clone(),
    };
    let started = Instant::now();
    let stream = source.start(sink.clone())?;
    while sink.status.load(Ordering::Acquire) == 0
        && !controls.stop.load(Ordering::Acquire)
        && !controls.cancel.load(Ordering::Acquire)
        && started.elapsed() < limit
    {
        thread::sleep(Duration::from_millis(5));
    }
    controls.stop.store(true, Ordering::Release);
    drop(stream);
    if controls.cancel.load(Ordering::Acquire) {
        return Err(Error::Cancelled);
    }
    match sink.status.load(Ordering::Acquire) {
        2 => return Err(Error::Format),
        3 => return Err(Error::Overrun),
        4 => return Err(Error::DeviceLost),
        _ => {}
    }
    let mut buffer = sink.buffer.lock().map_err(|_| Error::Overrun)?;
    buffer.take().map(Mono::finish).ok_or(Error::Unavailable)
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Guard(Arc<AtomicBool>);
    impl Drop for Guard {
        fn drop(&mut self) {
            self.0.store(true, Ordering::Release);
        }
    }
    struct Fixture {
        dropped: Arc<AtomicBool>,
        action: fn(Sink),
    }
    impl Source for Fixture {
        type Stream = Guard;
        fn format(&self) -> Result<(u32, u16), Error> {
            Ok((8000, 1))
        }
        fn start(&self, sink: Sink) -> Result<Guard, Error> {
            (self.action)(sink);
            Ok(Guard(self.dropped.clone()))
        }
    }
    #[test]
    fn stream_drops_on_duration_stop_cancel_device_loss_and_overrun() {
        for (action, expected) in [
            ((|_: Sink| {}) as fn(Sink), None),
            (
                |sink| {
                    sink.push(&[0.5f32]);
                    sink.controls.stop.store(true, Ordering::Release);
                },
                None,
            ),
            (
                |sink| sink.controls.cancel.store(true, Ordering::Release),
                Some(Error::Cancelled),
            ),
            (|sink| sink.fail(Error::DeviceLost), Some(Error::DeviceLost)),
            (
                |sink| {
                    let _guard = sink.buffer.lock().unwrap();
                    sink.push(&[1i16]);
                },
                Some(Error::Overrun),
            ),
        ] {
            let dropped = Arc::new(AtomicBool::new(false));
            let controls = Controls::default();
            let result = run_until(
                &Fixture {
                    dropped: dropped.clone(),
                    action,
                },
                controls.clone(),
                Duration::from_millis(10),
            );
            assert_eq!(result.err(), expected);
            assert_eq!(
                controls.overruns.load(Ordering::Acquire),
                u64::from(expected == Some(Error::Overrun))
            );
            assert!(dropped.load(Ordering::Acquire));
        }
    }
    #[test]
    fn full_buffer_stops_fixture_and_returns_exactly_sixty_seconds() {
        let dropped = Arc::new(AtomicBool::new(false));
        let fixture = Fixture {
            dropped: dropped.clone(),
            action: |sink| {
                let block = [0i16; 8000];
                for _ in 0..61 {
                    sink.push(&block);
                }
            },
        };
        let controls = Controls::default();
        let audio = run(&fixture, controls.clone()).unwrap();
        assert_eq!(controls.samples.load(Ordering::Acquire), 480000);
        assert_eq!(audio.samples.len(), 480000);
        assert_eq!(audio.sample_rate, 8000);
        assert!(dropped.load(Ordering::Acquire));
    }
}
