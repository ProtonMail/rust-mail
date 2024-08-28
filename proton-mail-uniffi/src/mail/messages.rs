//! Functions for working with [`Message`]s.
//!
//! The functions presented here can operate in one of two scopes: either on a
//! [`Mailbox`], or on a [`MailSession`]. The difference is that operations that
//! rely on the context of a mailbox/label view are performed on a mailbox, and
//! operations that are more global in nature are performed on a session. The
//! scope of the methods might change over time, but their primary association
//! of working with messages, and hence their placement in this module, won't.
//!

use super::datatypes::{BlockQuote, RemoteContent};
use super::datatypes::{MessageAvailableAction, MimeType};
use super::{MailUserSession, Mailbox, MailboxResult};
use crate::core::datatypes::Id;
use crate::mail::datatypes::{Message, MessageSearchOptions};
use crate::mail::{MailSessionError, MailboxError};
use crate::{uniffi_async, watch, LiveQueryCallback, WatchHandle};
use indoc::formatdoc;
use itertools::Itertools as _;
use proton_api_core::session::CoreSession;
use proton_core_common::datatypes::{Id as RealId, LabelId as RealLabelId, LocalId as RealLocalId};
use proton_mail_common::decrypted_message::{
    self, BodyOutput as RealBodyOutput, DecryptedMessageBody,
};
use proton_mail_common::models::{self, Label as RealLabel, MailSettings, Message as RealMessage};
use proton_mail_common::MailUserContext;
use stash::orm::Model as _;
use stash::params;
use std::sync::Arc;
use tokio::task::JoinError;

/// Which transform options to apply to the html.
///
/// Most transforms are either implicit, mandatory or read from the settings.
#[derive(Debug, Clone, Copy, Default, uniffi::Record)]
pub struct TransformOpts {
    pub block_quote: BlockQuote,
    pub remote_content: RemoteContent,
}

#[derive(Clone, uniffi::Object)]
pub struct DecryptedMessage {
    pub(crate) ctx: Arc<MailUserContext>,
    pub(crate) body: DecryptedMessageBody,
}

/// The result of transforming the message body.
/// It will have more things in the future
#[non_exhaustive]
#[derive(Debug, Clone, uniffi::Record)]
/// The result of transforming the message body.
pub struct BodyOutput {
    /// The transformed html of the message.
    pub body: String,

    /// Whether or not [`RemoteContent::Strip`] removed a blockquote.
    pub had_blockquote: bool,

    /// How many html tags it has removed.
    pub tags_stripped: u64,

    /// How many UTM tracking params it has removed.
    pub utm_stripped: u64,
}

#[uniffi::export]
impl DecryptedMessage {
    /// Gets the message body as an HTML. This does all of the transformations that are
    /// required based on the options and the user settings.
    ///
    /// # Parameters
    ///
    /// * `opts`: Which transform to apply to the html.
    ///
    /// # Errors
    ///
    /// Returns an error if the network request, the database query, reading/writing
    /// the body to the cache, or decrypting the body fails,
    /// or if the message doesn't exist.
    pub async fn body(&self, opts: TransformOpts) -> Result<BodyOutput, MailboxError> {
        let user_ctx = self.ctx.clone();
        let mail_settings = uniffi_async::<_, JoinError, _>(async move {
            let mail_settings = MailSettings::get(&user_ctx.user_stash().into())
                .await
                .unwrap_or_default()
                .unwrap_or_default();
            Ok(mail_settings)
        })
        .await?;
        let user_session_id = self.ctx.user_id();
        let RealBodyOutput {
            body,
            had_blockquote,
            tags_stripped,
            utm_stripped,
        } = decrypted_message::transform_html(
            &self.body.body,
            opts.remote_content.into(),
            opts.block_quote.into(),
            &mail_settings,
            user_session_id,
        );
        Ok(BodyOutput {
            body,
            had_blockquote,
            tags_stripped,
            utm_stripped,
        })
    }

    #[must_use]
    /// Retrieve a parsed header value for a given `key`.
    /// Returns a (possibly empty) array of header values.
    pub fn parsed_header_value(&self, key: &str) -> Vec<String> {
        match self.body.parsed_header_value(key) {
            Some(decrypted_message::ParsedHeaderValue::Array(arr)) => arr,
            Some(decrypted_message::ParsedHeaderValue::String(s)) => vec![s],
            None => vec![],
        }
    }

    #[must_use]
    /// Get the mime type from this message
    pub fn mime_type(&self) -> MimeType {
        self.body.metadata.mime_type.into()
    }

    #[must_use]
    /// This is `Some` if the message is multipart. It contains the subject (if it has it) and the
    /// attachments.
    pub fn get_multipart_data(&self) -> Option<MultipartData> {
        let attachments = self
            .body
            .pgp_attachments
            .clone()?
            .into_iter()
            .map(|x| PgpAttachment {
                id: x.id,
                content_id: x.content_id,
                name: x.name,
                size: x.size as u64,
                mime_type: x.mime_type,
                data: x.data,
            })
            .collect_vec();

        let subject = self.body.pgp_subject.clone();
        Some(MultipartData {
            subject,
            attachments,
        })
    }
}

/// This comes from a multipart message, not to be confused with the other attachments.
#[derive(Debug, PartialEq, Eq, Clone, Hash, uniffi::Record)]
pub struct PgpAttachment {
    /// Unique id across all attachments in an inbox.
    pub id: String,
    /// Content id extracted from mime.
    pub content_id: String,
    /// File name of the attachment.
    pub name: String,
    /// The size of the attachment in bytes.
    pub size: u64,
    /// The content type of the attachment.
    ///
    /// Is an empty string if no content type was found.
    pub mime_type: String,
    /// The attachment data.
    pub data: Vec<u8>,
}

/// The extra data of a multipart message.
#[derive(Debug, PartialEq, Eq, Clone, Hash, uniffi::Record)]
pub struct MultipartData {
    /// The subject that comes from a multipart message.
    subject: Option<String>,
    /// Attachments that come from a multipart message.
    attachments: Vec<PgpAttachment>,
}

/// Get a specified message.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `id`       - The local ID of the message to get.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn message(
    session: Arc<MailUserSession>,
    id: Id,
) -> Result<Option<Message>, MailboxError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move { Ok(RealMessage::load(id.into(), &stash).await?.map(Into::into)) })
        .await
}

/// Get messages for the given conversation.
///
/// # Parameters
///
/// * `session`         - The session to use for the request.
/// * `conversation_id` - The local ID of the conversation to get messages for.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn messages_for_conversation(
    session: Arc<MailUserSession>,
    conversation_id: Id,
) -> Result<Vec<Message>, MailboxError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        Ok(RealMessage::find(
            "WHERE local_conversation_id = ?",
            params![RealLocalId::from(conversation_id)],
            &stash,
            None,
        )
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
    })
    .await
}

/// Get messages for the given label.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `label_id` - The local ID of the label to get messages for.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn messages_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
) -> Result<Vec<Message>, MailboxError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        Ok(RealMessage::find(
            formatdoc!(
                "
                JOIN message_labels
                    ON messages.local_id = message_labels.local_message_id
                WHERE
                    message_labels.local_label_id = ?
                "
            ),
            params![RealLocalId::from(label_id)],
            &stash,
            None,
        )
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
    })
    .await
}

/// Filter or search messages which match the specified options.
///
/// Note that search results are inserted into the database.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `options` - The search options to use.
///
/// # Errors
///
/// Returns an error if the network request or database query fails.
///
#[uniffi::export]
pub async fn search_for_messages(
    session: Arc<MailUserSession>,
    options: MessageSearchOptions,
) -> Result<Vec<Message>, MailSessionError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        Ok(RealMessage::search(
            options.into_api_options(&stash).await?,
            session.ctx().session().api(),
            &stash,
        )
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
    })
    .await
}

/// Returns available actions for message
/// Any action returned here should impact current state of the message
/// and also should be available for the user to perform.
/// There is no need for any additional calculations before executing them.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `id`      - The local ID of the message to retrieve.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn available_actions_for_message(
    session: Arc<MailUserSession>,
    id: Id,
) -> MailboxResult<Vec<MessageAvailableAction>> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        let Some(message) = RealMessage::load(id.into(), &stash).await? else {
            return Ok(vec![]);
        };
        Ok(message
            .available_actions(&stash)
            .await?
            .into_iter()
            .map_into()
            .collect())
    })
    .await
}
/// Return the decrypted body of the specified message.
///
/// If the message body has never been fetched before, it will be retrieved from
/// the servers.
/// Obtains a [`DecryptedMessage`] given a message id.
#[uniffi::export]
pub async fn get_message_body(mbox: &Mailbox, id: Id) -> MailboxResult<DecryptedMessage> {
    let ctx = mbox.mbox().user_context();
    uniffi_async(async move {
        let body = models::Message::message_body(&ctx, id.into()).await?;
        Ok(DecryptedMessage { ctx, body })
    })
    .await
}

/// Data for watched messages.
#[derive(uniffi::Record)]
pub struct WatchedMessages {
    /// The messages.
    pub messages: Vec<Message>,

    /// The handle to stop watching the messages.
    pub handle: Arc<WatchHandle>,
}

/// Watch messages for the given label.
///
/// Watches messages with the specified label for changes. When the messages
/// change, the callback will be invoked.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `label_id` - The local ID of the label to watch.
/// * `callback` - The callback to use for updates. When the specified messages
///                change, the callback will be invoked.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn watch_messages_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedMessages, MailboxError> {
    let stash = session.user_stash().clone();
    uniffi_async(async move {
        let remote_label_id = RealLabelId::from(
            RealLocalId::from(label_id)
                .counterpart::<RealLabel, _>(&stash)
                .await?
                .unwrap(),
        );
        let (messages, handle) = watch::<_, _, RealMessage>(
            formatdoc!(
                "
                JOIN message_labels
                    ON messages.local_id = message_labels.local_message_id
                WHERE
                    message_labels.local_label_id = ?
                "
            ),
            params![RealLocalId::from(label_id)],
            move |r| r.label_ids.contains(&remote_label_id),
            |r| r.local_id.expect("local_id should never be None"),
            &stash,
            Arc::new(callback),
        )
        .await?;
        Ok(WatchedMessages {
            messages: messages.into_iter().map(Into::into).collect(),
            handle,
        })
    })
    .await
}
