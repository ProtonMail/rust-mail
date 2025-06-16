use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

use parking_lot::Mutex;

pub struct CoreClock {
    created: Instant,
    auto_lock_accessed: ActivityClock,
    pin_code_accessed: ActivityClock,
}

impl Default for CoreClock {
    fn default() -> Self {
        Self::new()
    }
}

impl CoreClock {
    #[must_use]
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            created: now,
            auto_lock_accessed: ActivityClock::new(now),
            pin_code_accessed: ActivityClock::new(now),
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.created.elapsed()
    }

    pub fn auto_lock_elapsed(&self) -> Duration {
        self.auto_lock_accessed.elapsed()
    }

    pub fn pin_code_elapsed(&self) -> Duration {
        self.pin_code_accessed.elapsed()
    }

    pub fn auto_lock_tick(&self) {
        self.auto_lock_accessed.tick();
    }

    pub fn pin_code_tick(&self) {
        self.pin_code_accessed.tick();
        self.pin_code_accessed
            .accessed
            .store(true, Ordering::Release);
    }

    pub fn auto_lock_accessed(&self) {
        self.auto_lock_accessed
            .accessed
            .store(true, Ordering::Release);
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl CoreClock {
    pub fn pin_code_duration_sub(&self, duration: Duration) {
        self.pin_code_accessed.sub(duration);
    }

    pub fn auto_lock_duration_sub(&self, duration: Duration) {
        self.auto_lock_accessed.sub(duration);
    }
}

pub struct ActivityClock {
    last_activity: Mutex<Instant>,
    accessed: AtomicBool,
}

impl ActivityClock {
    #[must_use]
    pub fn new(now: Instant) -> Self {
        Self {
            last_activity: Mutex::new(now),
            accessed: AtomicBool::new(true),
        }
    }

    pub fn tick(&self) {
        if self.accessed.swap(false, Ordering::Acquire) {
            *self.last_activity.lock() = Instant::now();
        }
    }

    pub fn elapsed(&self) -> Duration {
        self.last_activity.lock().elapsed()
    }

    pub fn sub(&self, duration: Duration) {
        let mut last_activity = self.last_activity.lock();
        *last_activity = last_activity
            .checked_sub(duration)
            .unwrap_or(*last_activity);
    }
}
