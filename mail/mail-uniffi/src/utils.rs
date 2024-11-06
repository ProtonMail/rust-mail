use std::{
    iter::{Cycle, StepBy},
    ops::RangeInclusive,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock,
    },
    time::Duration,
};

use tokio::{sync::Mutex, task, time::interval};

use crate::LiveQueryCallback;

/// Period of delay for dampening, in milliseconds. Each set of updates will be
/// held back for up until this amount of time before the callback is triggered
/// to notify the client.
pub const MIN_DAMPENING_PERIOD: u64 = 100;
pub const MAX_DAMPENING_PERIOD: u64 = 200;

struct Dampening {
    iter: Cycle<StepBy<RangeInclusive<u64>>>,
}

impl Iterator for Dampening {
    type Item = u64;

    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next()
    }
}

impl Dampening {
    fn new() -> Self {
        let iter = (MIN_DAMPENING_PERIOD..=MAX_DAMPENING_PERIOD)
            .step_by(10)
            .cycle();

        Self { iter }
    }
}

static DAMPENING_PERIOD: LazyLock<Mutex<Dampening>> =
    LazyLock::new(|| Mutex::new(Dampening::new()));

/// Obtains dampening function.
///
/// This returns a function that updates the boolean flag of whether we should
/// send an update which gets checked every `duration`.
///
/// Like [`damp()`] but takes a custom duration.
///
pub fn damp_with_duration(
    callback: Box<dyn LiveQueryCallback>,
    duration: Duration,
) -> impl Fn() + Clone {
    let must_update = Arc::new(AtomicBool::new(false));
    let must_update_weak = Arc::downgrade(&must_update);

    tokio::spawn(async move {
        let mut interval = interval(duration);
        let callback = Arc::new(callback);

        loop {
            interval.tick().await;
            let Some(must_update) = must_update_weak.upgrade() else {
                return;
            };
            // If there's something in there we call on_update and set false
            // If there isn't we set false either way
            if must_update.swap(false, Ordering::Relaxed) {
                let callback_clone = callback.clone();
                interval.tick().await;

                if task::spawn_blocking(move || callback_clone.on_update())
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }
    });

    move || must_update.store(true, Ordering::Relaxed)
}

/// Dampens the amount of noise, i.e. number of notifications, passed through.
///
/// Reduces how often the given notification callback gets called to a maximum
/// of once every dampening period.
///
/// It returns the function to use to actually notify the client, which you can
/// call as often as you want.
///
pub async fn damp(callback: Box<dyn LiveQueryCallback>) -> impl Fn() + Clone {
    let dampening_period = DAMPENING_PERIOD.lock().await.next().unwrap();
    damp_with_duration(callback, Duration::from_millis(dampening_period))
}
