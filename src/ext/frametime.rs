//! EXT: frame-time statistics for the dev screenshot path. Not part of the C++ port.
//!
//! The engine has no timing output of its own and vsync is hard-on, so until this existed the
//! only way to know what a frame cost was to count screenshots against a stopwatch. `--no-vsync`
//! (main.rs) removes the display cap; this records the wall time between successive frames and
//! reports the average and the 95th percentile once `--shot` fires, so a change can be measured
//! with one command and compared against the last one.
//!
//! The first frames are excluded: the scene's first frame pays for shader warm-up and buffer
//! uploads, and the next few settle physics and the door animation. Including them would make
//! every run's numbers depend on how long the driver took to compile, which is not the thing
//! being optimised.

use std::time::Instant;

/// Frames dropped from the front of the record before statistics are taken.
pub const WARMUP: usize = 10;

pub struct FrameClock {
    last: Option<Instant>,
    /// Seconds per frame, in order.
    times: Vec<f32>,
}

impl FrameClock {
    pub fn new() -> FrameClock {
        FrameClock { last: None, times: Vec::new() }
    }

    /// Call once per rendered frame, at the same point in the frame each time. The interval
    /// between two calls is one whole frame -- render, swap and event pump included -- which is
    /// the number the display cap is compared against.
    pub fn tick(&mut self) {
        let now = Instant::now();
        if let Some(last) = self.last {
            self.times.push((now - last).as_secs_f32());
        }
        self.last = Some(now);
    }

    /// EXT: mean milliseconds over the last few frames, for the developer overlay
    /// (src/ext/debug.rs). A short window rather than `stats`'s whole record, because a
    /// readout is asked "what is it costing me now?", and an average taken over a session
    /// that began in another scene answers a different question. `None` until the warm-up
    /// frames are behind us, so the number never opens on a shader compile.
    pub fn recent_ms(&self) -> Option<f32> {
        /// Frames in the window. A third of a second at 60 Hz: long enough that the last
        /// digit is not a flicker, short enough to react while you are still standing there.
        const WINDOW: usize = 20;
        let past = self.times.get(WARMUP..)?;
        let tail = past.get(past.len().saturating_sub(WINDOW)..)?;
        if tail.is_empty() {
            return None;
        }
        Some(tail.iter().sum::<f32>() / tail.len() as f32 * 1000.0)
    }

    /// `(avg ms, p95 ms, frames counted)` over everything after the warm-up, or `None` if
    /// nothing has been recorded past it.
    pub fn stats(&self) -> Option<(f32, f32, usize)> {
        summarize(self.times.get(WARMUP..).unwrap_or(&[]))
    }
}

/// Average and 95th percentile of a set of frame times, in milliseconds.
///
/// The percentile is the nearest-rank definition: the value below which 95% of the frames fall.
/// Nearest rank rather than an interpolated quantile because with a few hundred samples the two
/// agree to a few microseconds, and this one is exact for the usual question -- "what did the
/// slowest one-in-twenty frame cost?"
pub fn summarize(secs: &[f32]) -> Option<(f32, f32, usize)> {
    if secs.is_empty() {
        return None;
    }
    let n = secs.len();
    let avg = secs.iter().sum::<f32>() / n as f32;
    let mut sorted: Vec<f32> = secs.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("frame time is not NaN"));
    let rank = ((0.95 * n as f32).ceil() as usize).clamp(1, n);
    Some((avg * 1e3, sorted[rank - 1] * 1e3, n))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_record_has_no_stats() {
        assert!(summarize(&[]).is_none());
        let c = FrameClock::new();
        assert!(c.stats().is_none());
    }

    #[test]
    fn average_and_p95_are_nearest_rank() {
        // 100 frames of 1 ms .. 100 ms: p95 by nearest rank is the 95th value.
        let secs: Vec<f32> = (1..=100).map(|i| i as f32 * 1e-3).collect();
        let (avg, p95, n) = summarize(&secs).unwrap();
        assert_eq!(n, 100);
        assert!((avg - 50.5).abs() < 1e-3, "avg {avg}");
        assert!((p95 - 95.0).abs() < 1e-3, "p95 {p95}");
        // A single frame is both its own average and its own p95.
        let (a, p, n) = summarize(&[0.004]).unwrap();
        assert_eq!(n, 1);
        assert!((a - 4.0).abs() < 1e-4 && (p - 4.0).abs() < 1e-4);
    }

    #[test]
    fn warmup_frames_are_excluded() {
        let mut c = FrameClock::new();
        // WARMUP + 1 ticks record WARMUP intervals, all of which are warm-up: no stats yet.
        for _ in 0..=WARMUP {
            c.tick();
        }
        assert!(c.stats().is_none());
        c.tick();
        assert_eq!(c.stats().unwrap().2, 1);
    }
}
