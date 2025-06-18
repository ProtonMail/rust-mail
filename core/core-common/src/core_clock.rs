use jiff::Zoned;
use parking_lot::{Mutex, RwLock};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Debug, Default)]
pub struct CoreClock {
    now: RwLock<Option<Zoned>>,
    auto_lock: ActivityClock,
    pin_code: ActivityClock,
}

impl CoreClock {
    /// Returns the current time -- or mocked time, if [`Self::pretend()`] was
    /// called before.
    pub fn now(&self) -> Zoned {
        self.now.read().clone().unwrap_or_else(Zoned::now)
    }

    /// Pretends that the current time is `now`.
    ///
    /// This function is supposed to be used only in tests.
    pub fn pretend(&self, now: Zoned) {
        *self.now.write() = Some(now);
    }

    pub fn auto_lock_elapsed(&self) -> Duration {
        if self.auto_lock.just_created.load(Ordering::Acquire) {
            Duration::ZERO
        } else {
            self.auto_lock.elapsed()
        }
    }

    pub fn pin_code_elapsed(&self) -> Duration {
        self.pin_code.elapsed()
    }

    pub fn auto_lock_tick(&self) {
        self.auto_lock.tick();
    }

    pub fn pin_code_tick(&self) {
        self.pin_code.tick();
        self.pin_code.accessed.store(true, Ordering::Release);
    }

    pub fn auto_lock_accessed(&self) {
        self.auto_lock.accessed.store(true, Ordering::Release);
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl CoreClock {
    pub fn pin_code_duration_sub(&self, duration: Duration) {
        self.pin_code.sub(duration);
    }

    pub fn auto_lock_duration_sub(&self, duration: Duration) {
        self.auto_lock.sub(duration);
    }
}

#[derive(Debug)]
pub struct ActivityClock {
    last_activity: Mutex<Instant>,
    accessed: AtomicBool,
    just_created: AtomicBool,
}

impl ActivityClock {
    pub fn tick(&self) {
        if self.accessed.swap(false, Ordering::Acquire) {
            *self.last_activity.lock() = Instant::now();
            self.just_created.store(false, Ordering::Release);
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

impl Default for ActivityClock {
    fn default() -> Self {
        Self {
            last_activity: Mutex::new(Instant::now()),
            accessed: AtomicBool::new(true),
            just_created: AtomicBool::new(true),
        }
    }
}
