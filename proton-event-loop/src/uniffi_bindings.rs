/// Generate an uniffi compatible uniffi types. Aside from the event EventLoop being generic, there is another
/// limitation of uniffi that prevents us from have the code split between this crate and the implementation crate.
/// Even though the [`SubscriberError`] and [`LoopEventError`] are concrete types, using these error types directly
/// in another crate cause an issue in the generated bindings where the generate code does not have access to the
/// implementation details.
///
/// To work around these, use the following macro to generate unique types tha can be safely exported. The example
/// below will generate the following uniffi types:
///
/// * TestEvenLoop
/// * TestLoopError
/// * TestLoopErrorHandlerReply
/// * TestLoopErrorHandler
/// ```
/// use proton_event_loop::gen_event_loop_uniffi_types;
/// use proton_api_core::declare_event;
///
/// declare_event!(MyEvent, {foo:i32});
/// gen_event_loop_uniffi_types!(Test, MyEvent);
///
///```
#[macro_export]
macro_rules! gen_event_loop_uniffi_types {
    ($name:ident, $event_type:ty) => {
        proton_event_loop::paste::paste! {
            #[derive(Debug, thiserror::Error)]
            #[cfg_attr(feature = "uniffi", derive(uniffi::Error))]
            #[cfg_attr(feature = "uniffi", uniffi(flat_error))]
            pub enum [<$name LoopError>] {
                #[error("Failed to read from store: {0}")]
                StoreRead(anyhow::Error),
                #[error("Failed to write store: {0}")]
                StoreWrite(anyhow::Error),
                #[error("Failed to retrieve event: {0}")]
                Provider(#[from] HttpRequestError),
                #[error("Subscriber ({0}) failed to apply event: {1}")]
                Subscriber(String, proton_event_loop::SubscriberError),
            }

            impl From<proton_event_loop::LoopError> for [<$name LoopError>] {
                fn from(v: proton_event_loop::LoopError) -> Self {
                    match v {
                        proton_event_loop::LoopError::StoreRead(e) => Self::StoreRead(e),
                        proton_event_loop::LoopError::StoreWrite(e) => Self::StoreWrite(e),
                        proton_event_loop::LoopError::Provider(e) => Self::Provider(e),
                        proton_event_loop::LoopError::Subscriber(s, e) => Self::Subscriber(s, e),
                    }
                }
            }

            /// Response returned by the `LoopErrorHandler` to control the behavior of the event loop after an error occurs.
            #[derive(Debug, Copy, Clone, Eq, PartialEq)]
            #[cfg_attr(feature = "uniffi", derive(uniffi::Enum))]
            pub enum [<$name LoopErrorHandlerReply>] {
                Pause,
                Retry,
                Abort,
            }

            impl From< [<$name LoopErrorHandlerReply>]> for proton_event_loop::LoopErrorHandlerReply {
                fn from(v: [<$name LoopErrorHandlerReply>]) -> Self {
                    match v {
                        [<$name LoopErrorHandlerReply>]::Abort => proton_event_loop::LoopErrorHandlerReply::Abort,
                        [<$name LoopErrorHandlerReply>]::Retry => proton_event_loop::LoopErrorHandlerReply::Retry,
                        [<$name LoopErrorHandlerReply>]::Pause => proton_event_loop::LoopErrorHandlerReply::Pause,
                    }
                }
            }


            #[uniffi::export(callback_interface)]
            pub trait [<$name LoopErrorHandler>]: Send + Sync {
                fn on_error(&self, error: [<$name LoopError>]) -> [<$name LoopErrorHandlerReply>];
            }

            struct [<Uniffi  $name LoopErrorHandler>](pub Box<dyn [<$name LoopErrorHandler>]>);

            impl proton_event_loop::LoopErrorHandler for [<Uniffi $name LoopErrorHandler>] {
                fn on_error(&self, error: proton_event_loop::LoopError) -> proton_event_loop::LoopErrorHandlerReply {
                    self.0.on_error(error.into()).into()
                }
            }

            #[derive(uniffi::Object, Clone)]
            pub struct [<$name EventLoop>](pub proton_event_loop::Loop<$event_type>);

            #[uniffi::export(async_runtime = "tokio")]
            impl [<$name EventLoop>] {

                #[uniffi::constructor]
                pub fn new() -> Arc<Self> {
                    let eloop = proton_event_loop::Loop::new();
                    Arc::new(Self(eloop))
                }

                pub fn resume(&self) {
                    self.0.resume()
                }

                pub fn pause(&self) {
                    self.0.pause()
                }

                pub async fn start_poller(&self,
                    session: &proton_api_core::uniffi_bindgen::Session,
                    error_handler: Box<dyn MailLoopErrorHandler>,
                ) -> Result<(), [<$name LoopError>]> {
                    let event_provider = proton_event_loop::ProtonProvider::new(session.0.clone());
                    let event_store = proton_event_loop::InMemoryStore::default();
                    let event_error_handler = [<Uniffi $name LoopErrorHandler>](error_handler);
                    //TODO: task handle?
                    self.0
                        .start(
                            Duration::from_secs(10),
                            Box::new(event_store),
                            Box::new(event_provider),
                            Box::new(event_error_handler),
                        )
                        .await?;
                    Ok(())
                }
            }
        }
    };
}
