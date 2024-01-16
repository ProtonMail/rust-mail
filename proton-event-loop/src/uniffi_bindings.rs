use crate::{LoopError, LoopErrorHandlerReply};
use std::sync::Arc;
use std::time::Duration;

#[derive(uniffi::Object, Clone)]
pub struct EventLoop(pub crate::Loop);

#[uniffi::export(callback_interface)]
pub trait LoopErrorHandler: Send + Sync {
    fn on_error(&self, error: LoopError) -> LoopErrorHandlerReply;
}

struct UniffiLoopErrorHandler(Box<dyn LoopErrorHandler>);

impl crate::LoopErrorHandler for UniffiLoopErrorHandler {
    fn on_error(&self, error: LoopError) -> LoopErrorHandlerReply {
        self.0.on_error(error)
    }
}

#[uniffi::export]
impl EventLoop {
    pub fn resume(&self) {
        self.0.resume()
    }

    pub fn pause(&self) {
        self.0.pause()
    }
}

#[uniffi::export(async_runtime = "tokio")]
pub async fn new_event_loop(
    client: &proton_api_rs::uniffi_bindgen::Client,
    session: &proton_api_rs::uniffi_bindgen::Session,
    error_handler: Box<dyn LoopErrorHandler>,
) -> Result<Arc<EventLoop>, LoopError> {
    let event_provider = crate::ProtonProvider::new(client.0.clone(), session.0.clone());
    let event_store = crate::InMemoryStore::default();
    let event_error_handler = UniffiLoopErrorHandler(error_handler);
    let eloop = crate::Loop::new();

    //TODO: task handle?
    eloop
        .start(
            Duration::from_secs(10),
            Box::new(event_store),
            Box::new(event_provider),
            Box::new(event_error_handler),
        )
        .await?;

    Ok(Arc::new(EventLoop(eloop)))
}
