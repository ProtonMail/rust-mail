use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::time::interval;

use crate::LiveQueryCallback;

/// Period of delay for dampening, in milliseconds. Each set of updates will be
/// held back for up until this amount of time before the callback is triggered
/// to notify the client.
const DAMPENING_PERIOD: u64 = 200;

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

        loop {
            interval.tick().await;
            let Some(must_update) = must_update_weak.upgrade() else {
                return;
            };
            // If there's something in there we call on_update and set false
            // If there isn't we set false either way
            if must_update.swap(false, Ordering::Relaxed) {
                callback.on_update();
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
pub fn damp(callback: Box<dyn LiveQueryCallback>) -> impl Fn() + Clone {
    damp_with_duration(callback, Duration::from_millis(DAMPENING_PERIOD))
}
