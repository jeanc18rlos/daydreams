//! Port of Timer.h.
//!
//! PORT: QueryPerformanceFrequency/QueryPerformanceCounter -> std::time::Instant.
//! The Win32 timer counts in QPC ticks whose rate is queried at construction; here the
//! "tick" is fixed at one nanosecond, so the frequency is the constant FREQUENCY below
//! (was: LARGE_INTEGER frequency; QueryPerformanceFrequency(&frequency), Timer.h:7/35).

use std::time::Instant;

pub struct Timer {
    // PORT: the C++ keeps `LARGE_INTEGER t1, t2` scratch fields that Start/Stop/StopStart
    // write into; only the epoch is needed here (was: LARGE_INTEGER t1, t2, Timer.h:36).
    start: Instant,
}

impl Timer {
    /// Ticks per second. Instant::elapsed is reported in nanoseconds.
    const FREQUENCY: i64 = 1_000_000_000;

    pub fn new() -> Timer {
        Timer { start: Instant::now() }
    }

    // PORT: `int64_t GetTicks()` is a non-const member because it stores into t2;
    // it takes &self here (was: int64_t GetTicks(), Timer.h:19-22).
    pub fn get_ticks(&self) -> i64 {
        self.start.elapsed().as_nanos() as i64
    }

    pub fn seconds_to_ticks(&self, s: f32) -> i64 {
        (Timer::FREQUENCY as f32 * s) as i64
    }

    // PORT: Start() / Stop() / StopStart() (Timer.h:10-17, 28-32) are dropped -- nothing
    // in the engine calls them; Engine.cpp only uses GetTicks and SecondsToTicks.
}
