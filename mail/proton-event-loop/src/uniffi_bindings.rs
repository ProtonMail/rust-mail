/// Generate an uniffi error loop handler type.
/// It's impossible with uniffi at the moment to use callbacks declared in other crates. Due to the generic nature
/// of the event loop, this macro aims to help with generating the required boilerplate to set this up.
/// ```
/// use proton_event_loop::{gen_event_loop_error_handler, LoopError, LoopErrorHandler, LoopErrorHandlerReply};
/// gen_event_loop_error_handler!(ExportedEventLoopName, WrapperName);
///```
#[macro_export]
macro_rules! gen_event_loop_error_handler {
    ($name:ident, $wrapper_name:ident) => {
        #[uniffi::export(callback_interface)]
        pub trait $name: Send + Sync {
            fn on_error(&self, error: LoopError) -> LoopErrorHandlerReply;
        }

        pub struct $wrapper_name(pub Box<dyn $name>);

        impl LoopErrorHandler for $wrapper_name {
            fn on_error(&self, error: LoopError) -> LoopErrorHandlerReply {
                self.0.on_error(error)
            }
        }
    };
}
