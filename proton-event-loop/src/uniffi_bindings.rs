use crate::{LoopError, LoopErrorHandlerReply};

#[uniffi::export(callback_interface)]
pub trait LoopErrorHandler: Send + Sync {
    fn on_error(&self, error: LoopError) -> LoopErrorHandlerReply;
}

pub struct UniffiLoopErrorHandler(pub Box<dyn LoopErrorHandler>);

impl crate::LoopErrorHandler for UniffiLoopErrorHandler {
    fn on_error(&self, error: LoopError) -> LoopErrorHandlerReply {
        self.0.on_error(error)
    }
}
