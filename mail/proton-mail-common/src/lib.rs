//! Everything Proton Mailbox related.
mod actions;
mod context;
pub mod db;
mod mailbox;

pub mod avatar;

mod proton_color;
pub mod settings;
mod user_context;

pub use context::*;
pub use mailbox::*;
pub use user_context::*;

// re-exports
pub use proton_api_mail;
pub use proton_core_common;

pub mod exports {
    pub use proton_action_queue;
    pub use proton_api_mail;
    pub use proton_api_mail::exports::*;
    pub use proton_core_common;
    pub use proton_event_loop;
    pub use proton_mail_html_transformer;
}

#[cfg(feature = "uniffi")]
uniffi::setup_scaffolding!();

#[cfg(feature = "uniffi")]
mod type_forwarding {
    // Required due to https://github.com/mozilla/uniffi-rs/issues/1988.

    uniffi::ffi_converter_forward!(
        proton_api_mail::domain::ConversationId,
        proton_api_mail::UniFfiTag,
        crate::UniFfiTag
    );

    uniffi::ffi_converter_forward!(
        proton_api_mail::domain::AttachmentId,
        proton_api_mail::UniFfiTag,
        crate::UniFfiTag
    );

    uniffi::ffi_converter_forward!(
        proton_api_mail::domain::LabelId,
        proton_api_mail::UniFfiTag,
        crate::UniFfiTag
    );

    uniffi::ffi_converter_forward!(
        proton_api_mail::domain::MessageId,
        proton_api_mail::UniFfiTag,
        crate::UniFfiTag
    );

    uniffi::ffi_converter_forward!(
        proton_api_mail::domain::ExternalId,
        proton_api_mail::UniFfiTag,
        crate::UniFfiTag
    );

    uniffi::ffi_converter_forward!(
        proton_api_mail::domain::MessageFlags,
        proton_api_mail::UniFfiTag,
        crate::UniFfiTag
    );

    uniffi::ffi_converter_forward!(
        proton_api_mail::proton_api_core::domain::AddressId,
        proton_api_mail::proton_api_core::UniFfiTag,
        crate::UniFfiTag
    );
}
