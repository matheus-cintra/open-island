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
const CALLBACK_LOCK_PATIENCE: Duration = Duration::from_millis(2);
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
        let deadline = Instant::now() + CALLBACK_LOCK_PATIENCE;
        let mut guard = loop {
            if let Ok(guard) = self.buffer.try_lock() {
                break guard;
            }
            if Instant::now() >= deadline {
                self.fail(Error::Overrun);
                return;
            }
            thread::yield_now();
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
#[derive(Clone, Copy)]
pub struct VoiceActivity {
    /// RMS at or above this counts as speech.
    pub speech: f32,
    /// Stop this long after speech falls back below the threshold.
    pub silence: Duration,
}
pub fn run(source: &impl Source, controls: Controls) -> Result<Audio, Error> {
    run_until(source, controls, Duration::from_secs(MAX_SECONDS as u64))
}
/// Like `run`, but reports the recent-window input level while the stream is live
/// and, with voice activity configured, stops on its own once speech has settled.
/// Callbacks fire from the polling loop, never from the audio callback thread.
pub fn run_with_levels(
    source: &impl Source,
    controls: Controls,
    activity: Option<VoiceActivity>,
    level: impl Fn(f32),
) -> Result<Audio, Error> {
    run_until_with(source, controls, Duration::from_secs(MAX_SECONDS as u64), activity, level)
}
fn run_until(source: &impl Source, controls: Controls, limit: Duration) -> Result<Audio, Error> {
    run_until_with(source, controls, limit, None, |_| {})
}
fn run_until_with(
    source: &impl Source,
    controls: Controls,
    limit: Duration,
    activity: Option<VoiceActivity>,
    level: impl Fn(f32),
) -> Result<Audio, Error> {
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
    // One eighth of a second of history per reading: responsive to speech,
    // insensitive to the callback's chunk sizes.
    let window = (rate as usize / 8).max(1);
    let interval = Duration::from_millis(60);
    let reading = interval.as_millis() as u64;
    let arm = 150;
    let mut next = interval;
    let mut spoken = 0u64;
    let mut quiet = 0u64;
    let mut floor = f32::MAX;
    let mut seen = 0u32;
    while sink.status.load(Ordering::Acquire) == 0
        && !controls.stop.load(Ordering::Acquire)
        && !controls.cancel.load(Ordering::Acquire)
        && started.elapsed() < limit
    {
        if started.elapsed() >= next {
            next += interval;
            // try_lock only: the audio callback owns this mutex and must never wait.
            let sampled = sink.buffer.try_lock().ok().and_then(|guard| {
                guard
                    .as_ref()
                    .map(|buffer| (!buffer.is_empty()).then(|| buffer.tail_level(window)))
            });
            match sampled {
                None => {}
                Some(None) => level(0.0),
                Some(Some(value)) => {
                    level(value);
                    if let Some(vad) = activity {
                        seen += 1;
                        if seen <= 5 {
                            // Learn the ambient floor before judging speech, so
                            // microphones whose noise sits above the absolute
                            // minimum still reach silence.
                            floor = floor.min(value);
                        } else {
                            if value >= (floor * 3.0).max(vad.speech) {
                                spoken += reading;
                                quiet = 0;
                            } else if spoken >= arm {
                                quiet += reading;
                                if quiet >= vad.silence.as_millis() as u64 {
                                    controls.stop.store(true, Ordering::Release);
                                }
                            }
                            if value < floor * 2.0 {
                                floor += (value - floor) * 0.1;
                            }
                        }
                    }
                }
            }
        }
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
    struct ConversationFixture {
        dropped: Arc<AtomicBool>,
        lead: usize,
        speech: usize,
        silence: usize,
        noise: f32,
    }
    impl Source for ConversationFixture {
        type Stream = Guard;
        fn format(&self) -> Result<(u32, u16), Error> {
            Ok((8000, 1))
        }
        fn start(&self, sink: Sink) -> Result<Guard, Error> {
            let (lead, speech, silence, noise) =
                (self.lead, self.speech, self.silence, self.noise);
            thread::spawn(move || {
                let voiced = [0.4f32; 800];
                let quiet = [noise; 800];
                for _ in 0..lead {
                    sink.push(&quiet);
                    thread::sleep(Duration::from_millis(80));
                }
                for _ in 0..speech {
                    sink.push(&voiced);
                    thread::sleep(Duration::from_millis(80));
                }
                for _ in 0..silence {
                    sink.push(&quiet);
                    thread::sleep(Duration::from_millis(80));
                }
            });
            Ok(Guard(self.dropped.clone()))
        }
    }
    #[test]
    fn voice_activity_stops_after_speech_settles_and_ignores_leading_silence() {
        let dropped = Arc::new(AtomicBool::new(false));
        let fixture = ConversationFixture {
            dropped: dropped.clone(),
            lead: 4,
            speech: 5,
            silence: 48,
            noise: 0.0,
        };
        let started = Instant::now();
        let audio = run_until_with(
            &fixture,
            Controls::default(),
            Duration::from_secs(6),
            Some(VoiceActivity {
                speech: 0.025,
                silence: Duration::from_millis(900),
            }),
            |_| {},
        )
        .unwrap();
        let elapsed = started.elapsed();
        assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
        assert!(audio.samples.len() > 1000, "{}", audio.samples.len());
        assert!(audio.samples.len() < 40_000, "{}", audio.samples.len());
        let dropped = Arc::new(AtomicBool::new(false));
        let fixture = ConversationFixture {
            dropped: dropped.clone(),
            lead: 4,
            speech: 0,
            silence: 48,
            noise: 0.0,
        };
        let started = Instant::now();
        let audio = run_until_with(
            &fixture,
            Controls::default(),
            Duration::from_millis(600),
            Some(VoiceActivity {
                speech: 0.025,
                silence: Duration::from_millis(200),
            }),
            |_| {},
        )
        .unwrap();
        assert!(started.elapsed() >= Duration::from_millis(500));
        assert!(audio.samples.len() >= 4000, "{}", audio.samples.len());
    }
    #[test]
    fn voice_activity_adapts_to_ambient_noise_above_the_floor() {
        let dropped = Arc::new(AtomicBool::new(false));
        let fixture = ConversationFixture {
            dropped: dropped.clone(),
            lead: 10,
            speech: 6,
            silence: 60,
            noise: 0.03,
        };
        let started = Instant::now();
        let audio = run_until_with(
            &fixture,
            Controls::default(),
            Duration::from_secs(6),
            Some(VoiceActivity {
                speech: 0.025,
                silence: Duration::from_millis(900),
            }),
            |_| {},
        )
        .unwrap();
        let elapsed = started.elapsed();
        assert!(elapsed < Duration::from_secs(5), "{elapsed:?}");
        assert!(audio.samples.len() > 1000, "{}", audio.samples.len());
        assert!(audio.samples.len() < 40_000, "{}", audio.samples.len());
    }
    #[test]
    fn live_levels_are_reported_while_streaming_and_stopped_with_it() {        let dropped = Arc::new(AtomicBool::new(false));
        let fixture = Fixture {
            dropped: dropped.clone(),
            action: |sink| sink.push(&[0.5f32; 800]),
        };
        let readings = Arc::new(Mutex::new(Vec::new()));
        let collected = readings.clone();
        run_until_with(
            &fixture,
            Controls::default(),
            Duration::from_millis(130),
            None,
            move |value| collected.lock().unwrap().push(value),
        )
        .unwrap();
        let collected = readings.lock().unwrap();
        assert!(!collected.is_empty());
        for value in collected.iter() {
            assert!((0.45..=0.55).contains(value));
        }
        drop(collected);
        let stopped = Arc::new(AtomicBool::new(false));
        let silenced = Fixture {
            dropped: stopped.clone(),
            action: |sink| {
                sink.push(&[0.5f32; 800]);
                sink.controls.stop.store(true, Ordering::Release);
            },
        };
        let after = Arc::new(Mutex::new(Vec::new()));
        let collected = after.clone();
        run_until_with(
            &silenced,
            Controls::default(),
            Duration::from_millis(130),
            None,
            move |value| collected.lock().unwrap().push(value),
        )
        .unwrap();
        assert!(after.lock().unwrap().is_empty());
    }
}
