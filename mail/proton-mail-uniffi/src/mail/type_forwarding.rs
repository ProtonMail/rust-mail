use proton_mail_common::proton_api_mail as papi_mail;
use proton_mail_common::proton_api_mail::proton_api_core as papi_core;
use proton_mail_common::proton_mail_db as pm_db;

// Required due to https://github.com/mozilla/uniffi-rs/issues/1988.

uniffi::ffi_converter_forward!(
    papi_core::domain::UserId,
    papi_core::UniFfiTag,
    crate::UniFfiTag
);

uniffi::ffi_converter_forward!(
    papi_core::domain::Uid,
    papi_core::UniFfiTag,
    crate::UniFfiTag
);

uniffi::ffi_converter_forward!(
    papi_mail::domain::ConversationId,
    papi_mail::UniFfiTag,
    crate::UniFfiTag
);

uniffi::ffi_converter_forward!(pm_db::LocalLabelId, pm_db::UniFfiTag, crate::UniFfiTag);
