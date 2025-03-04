/// When the action queue executes the action, it has to first check
/// whether the network is pressent, or the device is in the offline mode instead.
///
/// Thank's to this behavior, action queue executor does not try to
/// execute remote part of the action but waits for the signal
///
use async_trait::async_trait;

#[async_trait]
pub trait WaitForOnline: Send + Sync + 'static {
    /// Waits until network status is online
    async fn wait_for_online(&self);
}
