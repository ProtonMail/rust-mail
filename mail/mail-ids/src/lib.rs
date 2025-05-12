//! This is a temporary crate until the situation with our test setup is corrected.

// There is currently a problem when you declare these types in proton-mail-common
// and then import them in proton-mail-test-utils, which then is re-imported into
// proton-mail-common.
//
// This causes a compilation error where the compiler thinks that the type used in
// proton-mail-test-utils is different than the one in proton-mail-common.
// error[E0271]: type mismatch resolving `<[LocalMessageId; 1] as IntoIterator>::Item == LocalMessageId`
//     --> mail/mail-common/src/models/../tests/models/messages.rs:1821:24
//      |
// 1821 |     Message::mark_read([local_msg_id1], &tx)
//      |     ------------------ ^^^^^^^^^^^^^^^ expected `datatypes::LocalMessageId`, found `proton_mail_common::datatypes::LocalMessageId`
//      |     |
//      |     required by a bound introduced by this call
//      |
//      = note: `proton_mail_common::datatypes::LocalMessageId` and `datatypes::LocalMessageId` have similar names, but are actually distinct types
// note: `proton_mail_common::datatypes::LocalMessageId` is defined in crate `proton_mail_common`
//     --> /Users/user/Repos/proton-rust/mail/mail-common/src/datatypes.rs:2030:1
//      |
// 2030 | declare_local_id!(pub LocalMessageId => MessageId);
//      | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
// note: `datatypes::LocalMessageId` is defined in the current crate
//     --> mail/mail-common/src/datatypes.rs:2030:1
//      |
// 2030 | declare_local_id!(pub LocalMessageId => MessageId);
//      | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
//      = note: the crate `proton_mail_common` is compiled multiple times, possibly with different configurations
// note: required by a bound in `models::message::Message::mark_read`
//     --> mail/mail-common/src/models/message.rs:1949:32
//      |
// 1948 |     pub async fn mark_read(
//      |                  --------- required by a bound in this associated function
// 1949 |         ids: impl IntoIterator<Item = LocalMessageId>,
//      |                                ^^^^^^^^^^^^^^^^^^^^^ required by this bound in `Message::mark_read`
//      = note: this error originates in the macro `declare_local_id` (in Nightly builds, run with -Z macro-backtrace for more info)

use proton_core_common::declare_local_id;
use proton_mail_api::services::proton::common::{AttachmentId, ConversationId, MessageId};

declare_local_id!(pub LocalAttachmentId => AttachmentId);
declare_local_id!(pub LocalMessageId => MessageId);
declare_local_id!(pub LocalConversationId => ConversationId);
