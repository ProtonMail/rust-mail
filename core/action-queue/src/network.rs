use async_trait::async_trait;

/// When the action queue executes the action, it has to first check
/// whether the network is pressent, or the device is in the offline mode instead.
///
/// Thank's to this behavior, action queue executor does not try to
/// execute remote part of the action but waits for the signal
///
pub trait WaitForOnlineSubscribtion: Send + Sync + 'static {
    /// Returns a listener waiting for network to re-appear
    fn subscribe(&self) -> Box<dyn WaitForOnline>;
}

#[async_trait]
pub trait WaitForOnline: Send + Sync + 'static {
    /// Waits until network status is online
    async fn wait_for_online(&mut self);
}

/// Returns a dummy implementation of [`WaitForOnline`] that always returns
/// immediately
///
#[derive(Clone, Copy, Debug)]
pub struct DummyWaitForOnlineSubscribtion;

impl WaitForOnlineSubscribtion for DummyWaitForOnlineSubscribtion {
    fn subscribe(&self) -> Box<dyn WaitForOnline> {
        Box::new(DummyWaitForOnline)
    }
}

/// Dummy implementation of [`WaitForOnline`] that always returns
/// immediately
///
#[derive(Clone, Copy, Debug)]
pub struct DummyWaitForOnline;

#[async_trait::async_trait]
impl WaitForOnline for DummyWaitForOnline {
    async fn wait_for_online(&mut self) {}
}
