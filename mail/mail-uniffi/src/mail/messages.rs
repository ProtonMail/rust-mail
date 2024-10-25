//! Functions for working with [`Message`]s.
//!
//! The functions presented here can operate in one of two scopes: either on a
//! [`Mailbox`], or on a [`MailSession`]. The difference is that operations that
//! rely on the context of a mailbox/label view are performed on a mailbox, and
//! operations that are more global in nature are performed on a session. The
//! scope of the methods might change over time, but their primary association
//! of working with messages, and hence their placement in this module, won't.
//!

use super::datatypes::{AllBottomBarMessageActions, BlockQuote, Message, RemoteContent};
use super::datatypes::{LabelAsAction, MessageAvailableActions, MimeType, MoveAction};
use super::{MailSessionResult, MailUserSession, Mailbox, MailboxResult};
use crate::core::datatypes::Id;
use crate::core::paginator::MessagePaginator;
use crate::mail::datatypes::MessageSearchOptions;
use crate::mail::{MailSessionError, MailboxError};
use crate::utils::damp;
use crate::{uniffi_async, watch_channel, LiveQueryCallback, WatchHandle};
use crate::{PaginatorFilter, PaginatorSearchOptions};
use itertools::Itertools as _;
use proton_api_core::session::CoreSession;
use proton_core_common::datatypes::{LabelId as RealLabelId, LocalId as RealLocalId};
use proton_mail_common::datatypes::SystemLabelId;
use proton_mail_common::decrypted_message::{
    self, BodyOutput as RealBodyOutput, DecryptedMessageBody,
};
use proton_mail_common::models::{self, Label as RealLabel, Message as RealMessage};
use proton_mail_common::models::{
    PaginatorFilter as RealPaginatorFilter, PaginatorSearchOptions as RealPaginatorSearchOptions,
};
use proton_mail_common::MailUserContext;
use stash::orm::Model as _;
use std::sync::Arc;

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

impl From<RealBodyOutput> for BodyOutput {
    fn from(value: RealBodyOutput) -> Self {
        Self {
            body: value.body,
            had_blockquote: value.had_blockquote,
            tags_stripped: value.tags_stripped,
            utm_stripped: value.utm_stripped,
        }
    }
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
    pub async fn body(self: Arc<Self>, opts: TransformOpts) -> Result<BodyOutput, MailboxError> {
        let cloned = Arc::clone(&self);
        uniffi_async(async move {
            Ok(cloned
                .body
                .transformed(
                    &cloned.ctx,
                    opts.remote_content.into(),
                    opts.block_quote.into(),
                )
                .await?
                .into())
        })
        .await
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

/// Data for watched message.
#[derive(uniffi::Record)]
pub struct WatchedMessage {
    /// The message.
    pub message: Message,

    /// The handle to stop watching the messages.
    pub handle: Arc<WatchHandle>,
}

/// Watch message for changes.
///
/// When the messages change, the callback will be invoked.
///
/// Returns `None` if the message could not be found.
///
/// # Parameters
///
/// * `session`    - The session to use for the request.
/// * `message_id` - The local ID of the message to watch.
/// * `callback`   - The callback to use for updates. When the specified messages
///                change, the callback will be invoked.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn watch_message(
    session: Arc<MailUserSession>,
    message_id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<Option<WatchedMessage>, MailboxError> {
    let stash = session.user_stash().clone();
    let watcher = WatchHandle::default();
    let watcher_cloned = watcher.clone();
    uniffi_async(async move {
        let callback = damp(callback);
        let message = if let Some((message, receiver)) =
            RealMessage::watch_message(RealLocalId::from(message_id), &stash).await?
        {
            tokio::spawn(async move {
                loop {
                    if watcher_cloned.should_stop() {
                        return;
                    }

                    if receiver.recv_async().await.is_err() {
                        return;
                    }

                    callback();
                }
            });
            Some(message)
        } else {
            None
        };
        Ok(message.map(|m| WatchedMessage {
            message: m.into(),
            handle: Arc::new(watcher),
        }))
    })
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
        Ok(
            RealMessage::in_conversation(RealLocalId::from(conversation_id), &stash, None)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
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
        Ok(
            RealMessage::in_label(RealLocalId::from(label_id), &stash, None)
                .await?
                .into_iter()
                .map(Into::into)
                .collect(),
        )
    })
    .await
}

/// Paginate messages for the given label.
///
/// Gets a paginator for messages belonging to the specified label, which allows
/// navigation through the messages by page/window, and watches for changes.
/// When the messages change, the callback will be invoked.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `label_id` - The local ID of the label to watch.
/// * `filter`   - The filter options for pagination.
/// * `callback` - The callback to use for updates. When the specified messages
///                change, the callback will be invoked.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn paginate_messages_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
    filter: PaginatorFilter,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<MessagePaginator, MailboxError> {
    let context = session.ctx();
    let (msg_sender, msg_receiver) = flume::unbounded();
    uniffi_async(async move {
        let real_paginator = RealMessage::paginate_in_label(
            &context,
            RealLocalId::from(label_id),
            50,
            RealPaginatorFilter::from(filter),
            RealPaginatorSearchOptions::default(),
            true,
            Some(msg_sender),
        )
        .await?;
        Ok(MessagePaginator {
            real_paginator,
            handle: watch_channel(msg_receiver, callback),
        })
    })
    .await
}

/// Paginate messages returned from a search.
///
/// Gets a paginator for messages returned from a search, which allows
/// navigation through the messages by page/window, and watches for changes.
/// When the messages change, the callback will be invoked.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `options`  - The search options for pagination.
/// * `callback` - The callback to use for updates. When the specified messages
///                change, the callback will be invoked.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[allow(clippy::missing_panics_doc)]
#[uniffi::export]
pub async fn paginate_search(
    session: Arc<MailUserSession>,
    options: PaginatorSearchOptions,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<MessagePaginator, MailboxError> {
    let context = session.ctx();
    let (msg_sender, msg_receiver) = flume::unbounded();
    uniffi_async(async move {
        let real_paginator = RealMessage::paginate_in_label(
            &context,
            RealLocalId::from(RealLabelId::all_mail().parse::<u64>().unwrap()),
            50,
            RealPaginatorFilter::default(),
            RealPaginatorSearchOptions::from(options),
            false,
            Some(msg_sender),
        )
        .await?;
        Ok(MessagePaginator {
            real_paginator,
            handle: watch_channel(msg_receiver, callback),
        })
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

/// Returns available actions for messages.
/// Any action returned here should reflect the display needs.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `view`    - The local ID of the label which messages are viewed in.
/// * `ids`     - The local IDs of the messages to calcualte available actions for.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn available_actions_for_messages(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> MailboxResult<MessageAvailableActions> {
    uniffi_async(async move {
        let view = mailbox.mbox().label_id();
        let view = RealLabel::load(view, mailbox.stash())
            .await?
            .ok_or_else(|| MailboxError::LabelNotFound(view.into()))?;
        let actions = RealMessage::available_actions(
            view,
            ids.into_iter().map_into().collect(),
            mailbox.stash(),
        )
        .await?;

        Ok(MessageAvailableActions::from(actions))
    })
    .await
}

/// Returns available label_as actions for messages.
/// Any action returned here should reflect the display needs.
///
/// # Parameters
///
/// * `session` - The session to use for the request.
/// * `ids`     - The local IDs of the messages to calcualte available actions for.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn available_label_as_actions_for_messages(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> MailboxResult<Vec<LabelAsAction>> {
    uniffi_async(async move {
        let actions = RealMessage::available_label_as_actions(
            ids.into_iter().map_into().collect(),
            mailbox.stash(),
        )
        .await?
        .into_iter()
        .map_into()
        .collect_vec();

        Ok(actions)
    })
    .await
}

/// Returns available move_to actions for messages.
/// Any action returned here should reflect the display needs.
///
/// # Parameters
///
/// * `mailbox` - The current Mailbox.
/// * `ids`     - The local IDs of the messages to calcualte available actions for.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn available_move_to_actions_for_messages(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> MailboxResult<Vec<MoveAction>> {
    uniffi_async(async move {
        let view = mailbox.mbox().label_id();
        let view = RealLabel::load(view, mailbox.stash())
            .await?
            .ok_or_else(|| MailboxError::LabelNotFound(view.into()))?;
        let actions = RealMessage::available_move_to_actions(
            view,
            ids.into_iter().map_into().collect(),
            mailbox.stash(),
        )
        .await?
        .into_iter()
        .map_into()
        .collect_vec();

        Ok(actions)
    })
    .await
}

/// Returns available actions for messages bottom bar.
///
/// # Parameters
///
/// * `mailbox`     - The current Mailbox.
/// * `message_ids` - The local IDs of the messages to calculate available actions for.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi::export]
pub async fn all_available_bottom_bar_actions_for_messages(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
) -> MailboxResult<AllBottomBarMessageActions> {
    uniffi_async(async move {
        let actions = RealMessage::all_available_bottom_bar_actions_for_messages(
            mailbox.label_id().into(),
            message_ids.into_iter().map_into().collect(),
            mailbox.stash(),
        )
        .await?
        .into();
        Ok(actions)
    })
    .await
}

/// Return the decrypted body of the specified message.
///
/// If the message body has never been fetched before, it will be retrieved from
/// the servers.
/// Obtains a [`DecryptedMessage`] given a message id.
#[uniffi::export]
pub async fn get_message_body(mbox: &Mailbox, id: Id) -> MailSessionResult<DecryptedMessage> {
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
        let (messages, receiver) =
            RealMessage::watch_in_label(RealLocalId::from(label_id), &stash).await?;
        let watcher = watch_channel(receiver, callback);
        Ok(WatchedMessages {
            messages: messages.into_iter().map(Into::into).collect(),
            handle: watcher,
        })
    })
    .await
}

/// Label the given messages with the given label id.
///
/// # Parameters
///
/// * `session`     - The session to use for the request.
/// * `label_id`    - The local ID of the label to apply.
/// * `message_ids` - The local IDs of the messages to apply the label to.
///
/// # Errors
///
/// Returns an error if the action can not be executed.
///
#[uniffi::export]
pub async fn apply_label_to_messages(
    session: Arc<MailUserSession>,
    label_id: Id,
    message_ids: Vec<Id>,
) -> Result<(), MailSessionError> {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealMessage::action_apply_label(
            user_context.session(),
            user_context.queue(),
            label_id.into(),
            message_ids.into_iter().map(Into::into).collect(),
        )
        .await?;
        Ok(())
    })
    .await
}

/// Star the given messages.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `ids`      - The local IDs of the messages to apply the label to.
///
/// # Errors
///
/// Returns an error if the action can not be executed.
///
#[uniffi::export]
pub async fn star_messages(
    session: Arc<MailUserSession>,
    message_ids: Vec<Id>,
) -> Result<(), MailSessionError> {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealMessage::action_star(
            user_context.session(),
            user_context.queue(),
            message_ids.into_iter().map(Into::into).collect(),
        )
        .await?;
        Ok(())
    })
    .await
}

/// Unstar the given messages.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `ids`      - The local IDs of the messages to apply the label to.
///
/// # Errors
///
/// Returns an error if the action can not be executed.
///
#[uniffi::export]
pub async fn unstar_messages(
    session: Arc<MailUserSession>,
    message_ids: Vec<Id>,
) -> Result<(), MailSessionError> {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealMessage::action_unstar(
            user_context.session(),
            user_context.queue(),
            message_ids.into_iter().map(Into::into).collect(),
        )
        .await?;
        Ok(())
    })
    .await
}

/// Remove label from the given messages with the given label id.
///
/// # Parameters
///
/// * `session`     - The session to use for the request.
/// * `label_id`    - The local ID of the label to remove.
/// * `message_ids` - The local IDs of the messages to remove the label from.
///
/// # Errors
///
/// Returns an error if the action can not be executed.
///
#[uniffi::export]
pub async fn remove_label_from_messages(
    session: Arc<MailUserSession>,
    label_id: Id,
    message_ids: Vec<Id>,
) -> Result<(), MailSessionError> {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealMessage::action_remove_label(
            user_context.session(),
            user_context.queue(),
            label_id.into(),
            message_ids.into_iter().map(Into::into).collect(),
        )
        .await?;
        Ok(())
    })
    .await
}

/// Mark multiple messages as read.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `ids`      - The local IDs of the messages to mark as read.
///
/// # Errors
///
/// Returns an error if the action can not be executed.
///
#[uniffi::export]
pub async fn mark_messages_read(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
) -> Result<(), MailSessionError> {
    let user_context = mailbox.mbox().user_context();
    let label_id = mailbox.label_id();
    uniffi_async(async move {
        RealMessage::action_mark_read(
            user_context.session(),
            user_context.queue(),
            label_id.into(),
            message_ids.into_iter().map(Into::into).collect(),
        )
        .await?;
        Ok(())
    })
    .await
}

/// Mark multiple messages as unread.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `ids`      - The local IDs of the messages to mark as unread.
///
/// # Errors
///
/// Returns an error if the action can not be executed.
///
#[uniffi::export]
pub async fn mark_messages_unread(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
) -> Result<(), MailSessionError> {
    let user_context = mailbox.mbox().user_context();
    let label_id = mailbox.label_id();
    uniffi_async(async move {
        RealMessage::action_mark_unread(
            user_context.session(),
            user_context.queue(),
            label_id.into(),
            message_ids.into_iter().map(Into::into).collect(),
        )
        .await?;
        Ok(())
    })
    .await
}

/// Delete multiple messages
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `ids`      - The local IDs of the messages to delete.
///
/// # Errors
///
/// Returns an error if the action can not be executed.
///
#[uniffi::export]
pub async fn delete_messages(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
) -> Result<(), MailSessionError> {
    let user_context = mailbox.mbox().user_context();
    let label_id = mailbox.label_id();
    uniffi_async(async move {
        RealMessage::action_delete(
            user_context.session(),
            user_context.queue(),
            label_id.into(),
            message_ids.into_iter().map(Into::into).collect(),
        )
        .await?;
        Ok(())
    })
    .await
}

/// Gets the embedded attachment by cid for a message.
/// Returns None if it does not exist
///
/// # Parameters
///
/// * `mailbox`  - The current Mailbox.
/// * `id`       - The id of the message
/// * `cid`      - The cid of the attachment
///
#[uniffi::export]
pub async fn get_embedded_attachment(
    mailbox: Arc<Mailbox>,
    id: Id,
    cid: String,
) -> Result<Option<EmbeddedAttachmentInfo>, MailboxError> {
    uniffi_async(async move {
        let Some(att) =
            models::Message::get_embedded_attachment(mailbox.mbox(), id.into(), &cid).await?
        else {
            return Ok(None);
        };
        Ok(Some(EmbeddedAttachmentInfo {
            data: att.data,
            mime: att.mime,
            height: att.height,
            width: att.width,
        }))
    })
    .await
}

/// Struct returned by [`get_embedded_attachment`] representing the data of an embedded attachment.
#[derive(Clone, uniffi::Record)]
pub struct EmbeddedAttachmentInfo {
    /// The bytes of the attachment
    pub data: Vec<u8>,
    pub mime: String,
    pub height: Option<String>,
    pub width: Option<String>,
}

/// Change Labels of a list of messages and optionally archive them.
///
/// Set Labels from `selected_label_ids` while unsetting all those that are not in
/// `partially_selected_label_ids`.
///
/// # Parameters
///
/// * `mailbox`                      - Mailbox containing the messages.
/// * `message_ids`                  - List the ids of the messages to label.
/// * `selected_label_ids`           - List the ids of the Labels to set.
/// * `partially_selected_label_ids` - List the ids of the Labels to keep as is.
/// * `must_archive`                 - If true, the given messages will me move into Archive.
///
/// # Errors
///
/// Returns an error if the action can not be executed.
///
#[uniffi::export]
pub async fn label_messages_as(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
    selected_label_ids: Vec<Id>,
    partially_selected_label_ids: Vec<Id>,
    must_archive: bool,
) -> Result<bool, MailSessionError> {
    let user_context = mailbox.mbox().user_context();
    let source_label_id = mailbox.label_id();
    uniffi_async(async move {
        Ok(RealMessage::action_label_as(
            user_context.session(),
            user_context.queue(),
            source_label_id.into(),
            message_ids.into_iter().map_into().collect(),
            selected_label_ids.into_iter().map_into().collect(),
            partially_selected_label_ids
                .into_iter()
                .map_into()
                .collect(),
            must_archive,
        )
        .await?)
    })
    .await
}

/// Move given messages from a label into another.
///
/// # Parameters
///
/// * `session`        - The session to use for the request.
/// * `source_id`      - The local ID of the source label.
/// * `destination_id` - The local ID of the destination label.
/// * `message_ids`    - The local IDs of the messages to move.
///
/// # Errors
///
/// Returns an error if the action can not be executed.
///
#[uniffi::export]
pub async fn move_messages(
    session: Arc<MailUserSession>,
    source_id: Id,
    destination_id: Id,
    message_ids: Vec<Id>,
) -> Result<(), MailSessionError> {
    let user_context = session.ctx();
    uniffi_async(async move {
        RealMessage::action_move(
            user_context.session(),
            user_context.queue(),
            source_id.into(),
            destination_id.into(),
            message_ids.into_iter().map(Into::into).collect(),
        )
        .await?;
        Ok(())
    })
    .await
}
