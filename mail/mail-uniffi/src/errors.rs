mod action_error;
mod attachment_data_error;
mod draft_error;
mod error_reason;
mod event_error;
mod pin_error;
mod proton_error;
mod scroller_error;
mod session_error;
mod snooze_error;
pub(crate) mod unexpected;

pub use self::action_error::*;
pub use self::attachment_data_error::*;
pub use self::draft_error::*;
pub use self::error_reason::*;
pub use self::event_error::*;
pub use self::pin_error::*;
pub use self::proton_error::*;
pub use self::scroller_error::*;
pub use self::session_error::*;
pub use self::snooze_error::*;
use crate::mail::RsvpEvent;
use crate::mail::datatypes::MobileAction;
use crate::mail::messages::{AttachmentData, BodyOutput};

#[macro_export]
macro_rules! export_void_result {
    ($($(#[$meta:meta])* $name:ident($type:ty)),* $(,)?) => {$(
        #[allow(clippy::large_enum_variant)]
        #[allow(dead_code)]
        #[derive(uniffi::Enum)]
        pub enum $name {
            Ok,
            Error($type),
        }

        #[automatically_derived]
        impl<T, E> From<::std::result::Result<T, E>> for $name
        where
            E: Into<$type> + ::std::fmt::Debug,
        {
            fn from(value: ::std::result::Result<T, E>) -> Self {
                match value {
                    Ok(val) => Self::Ok,
                    Err(error) => {
                        ::tracing::error!("{error:?}");
                        Self::Error(error.into())
                    }
                }
            }
        }

        impl<E: Into<$type> + ::std::fmt::Debug> From<E> for $name {
            fn from(error: E) -> Self {
                Self::Error(error.into())
            }
        }
    )*};
}

#[macro_export]
macro_rules! export_typed_result {
    ($($(#[$meta:meta])* $name:ident($ok_type:ty, $err_type:ty)),* $(,)?) => {$(
        $(#[$meta])*
        #[allow(clippy::large_enum_variant)]
        #[allow(dead_code)]
        #[derive(uniffi::Enum)]
        pub enum $name {
            Ok($ok_type),
            Error($err_type),
        }

        #[automatically_derived]
        impl<T, E> From<::std::result::Result<T, E>> for $name
        where
            T: Into<$ok_type>,
            E: Into<$err_type> + ::std::fmt::Debug,
        {
            fn from(value: ::std::result::Result<T, E>) -> Self {
                match value {
                    Ok(val) => Self::Ok(val.into()),
                    Err(error) => {
                        ::tracing::error!("{error:?}");
                        Self::Error(error.into())
                    }
                }
            }
        }
    )*};
}

export_void_result! {
    VoidActionResult(ActionError),
    VoidDraftDiscardResult(DraftDiscardError),
    VoidDraftSendResult(DraftSendError),
    VoidDraftSaveResult(DraftSaveError),
    VoidDraftUndoSendResult(DraftUndoSendError),
    VoidEventResult(EventError),
    VoidProtonResult(ProtonError),
    VoidSessionResult(UserSessionError),
    VoidDraftPasswordResult(DraftPasswordError),
    VoidDraftExpirationResult(DraftExpirationError),
    VoidAnswerRsvpResult(ProtonError),
    VoidDraftAttachmentDispositionSwapResult(DraftAttachmentDispositionSwapError)
}

export_typed_result! {
    AttachmentDataResult(AttachmentData, AttachmentDataError),
    BodyOutputResult(BodyOutput, ProtonError),
    RsvpEventGetResult(RsvpEvent, ProtonError),
    MobileActionsResult(Vec<MobileAction>, ActionError),
}
