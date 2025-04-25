mod action_error;
pub(crate) mod api_service_error;
mod draft_error;
mod error_reason;
mod event_error;
mod login_error;
mod pin_error;
mod proton_error;
mod session_error;
pub(crate) mod unexpected;

use proton_mail_common::decrypted_message::BodyOutput;

use crate::mail::messages::EmbeddedAttachmentInfo;

pub use self::action_error::*;
pub use self::draft_error::*;
pub use self::error_reason::*;
pub use self::event_error::*;
pub use self::login_error::*;
pub use self::pin_error::*;
pub use self::proton_error::*;
pub use self::session_error::*;

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
    VoidDraftSaveSendResult(DraftSaveSendError),
    VoidDraftUndoSendResult(DraftUndoSendError),
    VoidEventResult(EventError),
    VoidLoginResult(LoginError),
    VoidProtonResult(ProtonError),
    VoidSessionResult(UserSessionError),
}

export_typed_result! {
    /// A common type to be shared between:
    /// - `Draft::get_embedded_attachment`,
    /// - `DecryptedMessage::get_embedded_attachment`.
    EmbeddedAttachmentInfoResult(EmbeddedAttachmentInfo, ProtonError),
    BodyOutputResult(BodyOutput, ProtonError)
}
