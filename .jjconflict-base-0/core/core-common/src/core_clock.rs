use crate::CoreContextError;
use crate::context::services::Service;
use async_trait::async_trait;
use jiff::Zoned;
use parking_lot::{Mutex, RwLock};
use std::{
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};
use tracing::{debug, instrument};

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
    #[cfg(feature = "test-utils")]
    pub fn pretend(&self, now: Zoned) {
        *self.now.write() = Some(now);
    }

    pub fn auto_lock_elapsed(&self) -> Option<Duration> {
        self.auto_lock.elapsed()
    }

    pub fn pin_code_elapsed(&self) -> Option<Duration> {
        self.pin_code.elapsed()
    }

    #[instrument(skip_all)]
    pub fn auto_lock_tick(&self) {
        self.auto_lock.tick();
    }

    #[instrument(skip_all)]
    pub fn pin_code_tick(&self) {
        self.pin_code.tick();
        self.pin_code.mark();
    }

    #[instrument(skip_all)]
    pub fn auto_lock_accessed(&self) {
        self.auto_lock.mark();
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl CoreClock {
    pub fn auto_lock_reset(&self) {
        self.auto_lock.reset();
    }

    pub fn pin_code_reset(&self) {
        self.pin_code.reset();
    }
}

#[derive(Debug, Default)]
struct ActivityClock {
    last_activity: Mutex<Option<Instant>>,
    accessed: AtomicBool,
}

impl ActivityClock {
    fn tick(&self) {
        let updated = if self.accessed.swap(false, Ordering::Acquire) {
            *self.last_activity.lock() = Some(Instant::now());
            true
        } else {
            false
        };

        debug!(?updated, "tick()");
    }

    fn mark(&self) {
        debug!("mark()");

        self.accessed.store(true, Ordering::Release);
    }

    fn elapsed(&self) -> Option<Duration> {
        self.last_activity.lock().map(|time| time.elapsed())
    }

    #[cfg(any(test, feature = "test-utils"))]
    pub fn reset(&self) {
        *self.last_activity.lock() = None;
    }
}

#[async_trait]
impl Service for CoreClock {
    type Error = CoreContextError;
}
