use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use tokio::time::interval;

use crate::LiveQueryCallback;

// This returns a function that updates the boolean flag of whether we should send an update which
// gets checked every `duration`.
/// Like [`damp`] but takes a custom duration.
pub fn damp_with_duration(callback: Box<dyn LiveQueryCallback>, duration: Duration) -> impl Fn() {
    let must_update = Arc::new(AtomicBool::new(false));
    let must_update_clone = must_update.clone();

    tokio::spawn(async move {
        let mut interval = interval(duration);

        loop {
            interval.tick().await;
            // If there's something in there we call on_update and set false
            // If there isn't we set false either way
            if must_update.swap(false, Ordering::Relaxed) {
                callback.on_update();
            }
        }
    });

    move || must_update_clone.store(false, Ordering::Relaxed)
}

/// Reduces how often the given notification callback gets called to max once per 5 seconds.
/// Returns the function you must use to actually notify the client, which you can call as often as
/// you want.
pub fn damp(callback: Box<dyn LiveQueryCallback>) -> impl Fn() {
    damp_with_duration(callback, Duration::from_secs(5))
}
