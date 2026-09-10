//! Live-output-stream error triage shared by every CPAL route (#660).
//!
//! cpal 0.17's ALSA worker reports `alsa::poll() returned POLLERR` and then
//! polls again immediately, without `snd_pcm_recover`: a PCM left in XRUN /
//! SUSPENDED / DISCONNECTED state (CPU starvation at a track change on a Pi,
//! a system suspend, a pulled USB cable) becomes a tight loop — measured at
//! ~1.7 M error callbacks per second on a Fosi ZH3 over `front:CARD=` — that
//! never plays again until the stream is dropped. rodio's default callback
//! `eprintln!`s every one of them, which is the "15,000 lines in a few
//! seconds" of issue #660.
//!
//! This module gives the routes one shared callback that (a) classifies the
//! error, (b) rate-limits what reaches the log, and (c) latches a WEDGED
//! verdict the audio thread polls so it can drop and rebuild the stream.

use rodio::cpal::StreamError;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// One log line per window from a flooding callback (cpal can fire ~1.7 M/s).
pub const LOG_WINDOW: Duration = Duration::from_secs(5);

/// What a live stream error means for the stream's future.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamFault {
    /// The PCM sits in an error state the worker never leaves on its own
    /// (the POLLERR loop, a device that is gone): drop the stream and
    /// rebuild it — cpal exposes no PCM handle, so that is the only recovery
    /// available from this side.
    Wedged(String),
    /// A one-off the worker recovers from itself (an EPIPE underrun goes
    /// through `try_recover`) or an error with no evidence of a stuck loop.
    Transient(String),
}

/// Classify a cpal stream error. Kept free of side effects so every route
/// (System default, ALSA, PipeWire, the legacy CPAL path) judges alike.
pub fn classify(err: &StreamError) -> StreamFault {
    let text = err.to_string();
    match err {
        StreamError::DeviceNotAvailable => StreamFault::Wedged(text),
        StreamError::BackendSpecific { err } if err.description.contains("POLLERR") => {
            StreamFault::Wedged(text)
        }
        _ => StreamFault::Transient(text),
    }
}

/// Admits at most one event per window and counts what it swallowed in
/// between, so a flood becomes "one line every 5 s with a suppressed count"
/// instead of one line per iteration of a spinning worker.
#[derive(Debug)]
pub struct FloodLimiter {
    window: Duration,
    last_admitted: Option<Instant>,
    suppressed: u64,
}

impl FloodLimiter {
    pub fn new(window: Duration) -> Self {
        Self {
            window,
            last_admitted: None,
            suppressed: 0,
        }
    }

    /// `Some(n)` when the caller should act on this event, where `n` is how
    /// many events were suppressed since the previous admitted one.
    pub fn admit(&mut self, now: Instant) -> Option<u64> {
        match self.last_admitted {
            Some(last) if now.duration_since(last) < self.window => {
                self.suppressed += 1;
                None
            }
            _ => {
                self.last_admitted = Some(now);
                Some(std::mem::take(&mut self.suppressed))
            }
        }
    }
}

/// Process-wide "the live output stream is wedged" latch. Raised from the
/// cpal worker thread by the shared callback, taken by the audio thread on
/// its idle tick. The first reason wins while latched; a rebuilt stream
/// starts from a cleared latch (its predecessor's worker is joined on drop,
/// so nothing raises it late).
static WEDGED: Mutex<Option<String>> = Mutex::new(None);

pub fn raise_wedged(reason: String) {
    if let Ok(mut latch) = WEDGED.lock() {
        if latch.is_none() {
            *latch = Some(reason);
        }
    }
}

pub fn take_wedged() -> Option<String> {
    WEDGED.lock().ok().and_then(|mut latch| latch.take())
}

pub fn clear_wedged() {
    if let Ok(mut latch) = WEDGED.lock() {
        *latch = None;
    }
}

/// The error callback every CPAL route installs
/// (`DeviceSinkBuilder::with_error_callback`). Logs the first error of a
/// burst and then one line per [`LOG_WINDOW`] with the suppressed count, and
/// latches a wedge exactly once per stream.
pub fn error_callback(route: &'static str) -> impl FnMut(StreamError) + Send + 'static {
    let mut limiter = FloodLimiter::new(LOG_WINDOW);
    let mut latched = false;
    move |err: StreamError| {
        let fault = classify(&err);
        if let Some(suppressed) = limiter.admit(Instant::now()) {
            if suppressed == 0 {
                log::warn!("[{route}] Stream error: {err}");
            } else {
                log::warn!(
                    "[{route}] Stream error: {err} ({suppressed} more suppressed in the last {}s)",
                    LOG_WINDOW.as_secs()
                );
            }
        }
        if let StreamFault::Wedged(reason) = fault {
            if !latched {
                latched = true;
                raise_wedged(format!("{route}: {reason}"));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::cpal::{BackendSpecificError, StreamError};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    /// The wedge latch is process-wide; serialise the tests that touch it.
    static LATCH_TESTS: Mutex<()> = Mutex::new(());

    fn pollerr() -> StreamError {
        StreamError::BackendSpecific {
            err: BackendSpecificError {
                description: "`alsa::poll()` returned POLLERR".to_string(),
            },
        }
    }

    #[test]
    fn pollerr_is_a_wedged_stream() {
        assert!(matches!(classify(&pollerr()), StreamFault::Wedged(_)));
    }

    #[test]
    fn device_loss_is_a_wedged_stream() {
        assert!(matches!(
            classify(&StreamError::DeviceNotAvailable),
            StreamFault::Wedged(_)
        ));
    }

    #[test]
    fn underrun_and_other_backend_errors_are_transient() {
        // cpal recovers an EPIPE underrun itself (try_recover); a one-off
        // backend error is not a reason to rebuild the stream.
        assert!(matches!(
            classify(&StreamError::BufferUnderrun),
            StreamFault::Transient(_)
        ));
        let other = StreamError::BackendSpecific {
            err: BackendSpecificError {
                description: "snd_pcm_writei returned EAGAIN".to_string(),
            },
        };
        assert!(matches!(classify(&other), StreamFault::Transient(_)));
    }

    #[test]
    fn limiter_admits_the_first_then_one_per_window_with_the_suppressed_count() {
        let t0 = Instant::now();
        let mut limiter = FloodLimiter::new(Duration::from_secs(5));
        assert_eq!(limiter.admit(t0), Some(0));
        for i in 1..=10_000u64 {
            assert_eq!(limiter.admit(t0 + Duration::from_micros(i)), None);
        }
        assert_eq!(limiter.admit(t0 + Duration::from_secs(5)), Some(10_000));
        assert_eq!(
            limiter.admit(t0 + Duration::from_secs(5) + Duration::from_millis(1)),
            None
        );
        assert_eq!(limiter.admit(t0 + Duration::from_secs(10)), Some(1));
    }

    #[test]
    fn wedged_latch_is_raised_once_and_taken_once() {
        let _guard = LATCH_TESTS.lock().unwrap();
        clear_wedged();
        assert_eq!(take_wedged(), None);
        raise_wedged("POLLERR".to_string());
        raise_wedged("a later reason".to_string()); // first reason wins while latched
        assert_eq!(take_wedged().as_deref(), Some("POLLERR"));
        assert_eq!(take_wedged(), None);
    }

    #[test]
    fn error_callback_latches_a_wedge_exactly_once_under_a_flood() {
        let _guard = LATCH_TESTS.lock().unwrap();
        clear_wedged();
        let mut callback = error_callback("test route");
        for _ in 0..100_000 {
            callback(pollerr());
        }
        assert!(take_wedged().is_some());
        assert_eq!(take_wedged(), None);
        // A transient error never latches.
        callback(StreamError::BufferUnderrun);
        assert_eq!(take_wedged(), None);
    }
}
