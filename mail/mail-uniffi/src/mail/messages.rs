//! Functions for working with [`Message`]s.
//!
//! The functions presented here can operate in one of two scopes: either on a
//! [`Mailbox`], or on a [`MailSession`]. The difference is that operations that
//! rely on the context of a mailbox/label view are performed on a mailbox, and
//! operations that are more global in nature are performed on a session. The
//! scope of the methods might change over time, but their primary association
//! of working with messages, and hence their placement in this module, won't.
//!

use super::datatypes::{AllBottomBarMessageActions, Message, ReadFilter, SearchScroller};
use super::datatypes::{LabelAsAction, MessageAvailableActions, MimeType, MoveAction};
use super::{MailUserSession, Mailbox};
use crate::PaginatorSearchOptions;
use crate::core::datatypes::{Id, RemoteId};
use crate::errors::{ActionError, EmbeddedAttachmentInfoResult, ProtonError, VoidActionResult};
use crate::mail::datatypes::MessageScroller;
use crate::mail::datatypes::MessageSearchOptions;
use crate::{LiveQueryCallback, WatchHandle, async_runtime, uniffi_async, watch_channel};
use itertools::Itertools as _;
use proton_core_common::datatypes::LocalLabelId;
use proton_core_common::models::Label as RealLabel;
use proton_core_common::utils::MapVec;
use proton_mail_common::MailUserContext;
use proton_mail_common::datatypes::LocalConversationId;
use proton_mail_common::decrypted_message::{
    self, BodyOutput, DecryptedMessageBody, TransformOpts,
};
use proton_mail_common::errors::{
    ActionErrorReason as RealActionErrorReason, ProtonMailError as RealProtonMailError,
};
use proton_mail_common::mail_scroller::MailScroller;
use proton_mail_common::models::default_location::IncomingDefaultLocation;
use proton_mail_common::models::{self, Message as RealMessage};
use stash::orm::Model as _;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(uniffi::Object)]
pub struct DecryptedMessage {
    pub(crate) ctx: Arc<MailUserContext>,
    pub(crate) body: DecryptedMessageBody,
}

#[uniffi_export]
impl DecryptedMessage {
    pub async fn body_with_defaults(self: Arc<Self>) -> BodyOutput {
        self.body(TransformOpts::default()).await
    }

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
    #[allow(clippy::missing_panics_doc)]
    pub async fn body(self: Arc<Self>, opts: TransformOpts) -> BodyOutput {
        async_runtime()
            .spawn(async move {
                let tether = self.ctx.user_stash().connection();
                self.body.transformed(opts, &tether).await
            })
            .await
            .expect("Transformed is infailable.")
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
    /// This is `Some` if the message is multipart and has a subject.
    pub fn get_pgp_subject(&self) -> Option<String> {
        self.body.pgp_subject.clone()
    }
}

#[uniffi_export]
impl DecryptedMessage {
    /// Load or fetch an embedded attachment with `cid` for this message.
    ///
    /// If the attachment is not in the cache it will be downloaded from the server.
    ///
    /// # Errors
    ///
    /// Returns error if the attachments can't be fetched from the server, retrieved
    /// from the cache or the attachment with `cid` does not exist.
    //NOTE: iOS request we share the same result types between
    // this function and the Draft equivalent.
    #[returns(EmbeddedAttachmentInfoResult)]
    pub async fn get_embedded_attachment(
        self: Arc<Self>,
        cid: String,
    ) -> Result<EmbeddedAttachmentInfo, ProtonError> {
        uniffi_async(async move {
            let att = self
                .body
                .get_embedded_attachment(&self.ctx, &cid)
                .await
                .map_err(RealProtonMailError::from)?;
            Ok::<_, RealProtonMailError>(EmbeddedAttachmentInfo {
                data: att.data,
                mime: att.mime,
                height: att.height,
                width: att.width,
            })
        })
        .await
        .map_err(ProtonError::from)
        .into()
    }
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
#[uniffi_export]
pub async fn message(
    session: Arc<MailUserSession>,
    id: Id,
) -> Result<Option<Message>, ActionError> {
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        Result::<_, RealProtonMailError>::Ok(
            RealMessage::load(id.into(), &tether).await?.map(Into::into),
        )
    })
    .await
    .map_err(ActionError::from)
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
#[uniffi_export]
pub async fn watch_message(
    session: Arc<MailUserSession>,
    message_id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<Option<WatchedMessage>, ActionError> {
    let user_context = session.ctx()?;
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        let Some(message) = RealMessage::load(message_id.into(), &tether).await? else {
            return Ok(None);
        };
        let handle = RealMessage::watch(&stash)?;
        let handle = watch_channel(user_context, handle, callback);
        Result::<_, RealProtonMailError>::Ok(Some(WatchedMessage {
            message: message.into(),
            handle,
        }))
    })
    .await
    .map_err(ActionError::from)
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
#[uniffi_export]
pub async fn messages_for_conversation(
    session: Arc<MailUserSession>,
    conversation_id: Id,
) -> Result<Vec<Message>, ActionError> {
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        Result::<_, RealProtonMailError>::Ok(
            RealMessage::in_conversation(LocalConversationId::from(conversation_id), &tether)
                .await?
                .map_vec(),
        )
    })
    .await
    .map_err(ActionError::from)
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
#[uniffi_export]
pub async fn messages_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
) -> Result<Vec<Message>, ActionError> {
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        Ok::<_, RealProtonMailError>(
            RealMessage::in_label(LocalLabelId::from(label_id), &tether)
                .await?
                .map_vec(),
        )
    })
    .await
    .map_err(ActionError::from)
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
#[uniffi_export]
pub async fn scroll_messages_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
    filter: ReadFilter,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<Arc<MessageScroller>, ActionError> {
    let context = session.ctx()?;
    uniffi_async(async move {
        let mut scroller =
            MailScroller::messages(context.as_weak(), label_id.into(), filter.into(), 50).await?;
        let handle = scroller.watch()?;

        Result::<_, RealProtonMailError>::Ok(Arc::new(MessageScroller {
            scroller: Mutex::new(scroller),
            handle: watch_channel(context, handle, callback),
        }))
    })
    .await
    .map_err(ActionError::from)
}

/// Search for messages for the given keywords.
///
/// Gets a scroller for messages containing keywords, which allows
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
#[uniffi_export]
pub async fn scroller_search(
    session: Arc<MailUserSession>,
    options: PaginatorSearchOptions,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<Arc<SearchScroller>, ActionError> {
    let context = session.ctx()?;
    uniffi_async(async move {
        let mut scroller = MailScroller::search(context.as_weak(), options.into(), 50).await?;
        let handle = scroller.watch()?;

        Result::<_, RealProtonMailError>::Ok(Arc::new(SearchScroller {
            scroller: Mutex::new(scroller),
            handle: watch_channel(context, handle, callback),
        }))
    })
    .await
    .map_err(ActionError::from)
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
#[uniffi_export]
pub async fn search_for_messages(
    session: Arc<MailUserSession>,
    options: MessageSearchOptions,
) -> Result<Vec<Message>, ActionError> {
    let user_context = session.ctx()?;
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let mut tether = stash.connection();
        let messages = RealMessage::search(
            options.into_api_options(&tether).await?,
            user_context.api(),
            &mut tether,
        )
        .await?
        .into_iter()
        .map(Into::into)
        .collect();

        Result::<_, RealProtonMailError>::Ok(messages)
    })
    .await
    .map_err(ActionError::from)
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
#[uniffi_export]
pub async fn available_actions_for_messages(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<MessageAvailableActions, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let view = mailbox.mbox().label_id();
        let tether = stash.connection();
        let view = RealLabel::load(view, &tether)
            .await?
            .ok_or_else(|| RealProtonMailError::reason(RealActionErrorReason::UnknownLabel))?;
        let actions = RealMessage::available_actions(view, ids.map_vec(), &tether).await?;

        Ok::<_, RealProtonMailError>(MessageAvailableActions::from(actions))
    })
    .await
    .map_err(ActionError::from)
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
#[uniffi_export]
pub async fn available_label_as_actions_for_messages(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<Vec<LabelAsAction>, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        let actions = RealMessage::available_label_as_actions(ids.map_vec(), &tether)
            .await?
            .map_vec();

        Ok::<_, RealProtonMailError>(actions)
    })
    .await
    .map_err(ActionError::from)
}

/// Watches label_as actions for messages.
/// Any action returned here should reflect the display needs.
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `ids`      - The local IDs of the messages to calcualte available actions for.
/// * `callback` - The callback to use for updates.
///
/// # Errors
///
/// Returns an error if the database query fails.
///
#[uniffi_export]
pub async fn watch_available_label_as_actions_for_messages(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedLabelAs, ActionError> {
    let ctx = mailbox.ctx()?;
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        let (actions, handle) =
            RealMessage::watch_available_label_as_actions(ids.map_vec(), &tether).await?;
        let actions = actions.map_vec();
        let handle = watch_channel(ctx, handle, callback);

        Ok::<_, RealProtonMailError>(WatchedLabelAs { actions, handle })
    })
    .await
    .map_err(ActionError::from)
}

#[derive(Clone, uniffi::Record)]
pub struct WatchedLabelAs {
    pub actions: Vec<LabelAsAction>,
    pub handle: Arc<WatchHandle>,
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
#[uniffi_export]
pub async fn available_move_to_actions_for_messages(
    mailbox: Arc<Mailbox>,
    ids: Vec<Id>,
) -> Result<Vec<MoveAction>, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let view = mailbox.mbox().label_id();
        let tether = stash.connection();
        let view = RealLabel::load(view, &tether)
            .await?
            .ok_or_else(|| RealProtonMailError::reason(RealActionErrorReason::UnknownLabel))?;
        let actions = RealMessage::available_move_to_actions(
            view,
            ids.into_iter().map_into().collect(),
            &tether,
        )
        .await?
        .into_iter()
        .map_into()
        .collect_vec();

        Result::<_, RealProtonMailError>::Ok(actions)
    })
    .await
    .map_err(ActionError::from)
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
#[uniffi_export]
pub async fn all_available_bottom_bar_actions_for_messages(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
) -> Result<AllBottomBarMessageActions, ActionError> {
    let stash = mailbox.stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        let actions = RealMessage::all_available_bottom_bar_actions_for_messages(
            mailbox.label_id().into(),
            message_ids.map_vec(),
            &tether,
        )
        .await?
        .into();
        Ok::<_, RealProtonMailError>(actions)
    })
    .await
    .map_err(ActionError::from)
}

/// Return the decrypted body of the specified message.
///
/// If the message body has never been fetched before, it will be retrieved from
/// the servers.
/// Obtains a [`DecryptedMessage`] given a message id.
#[uniffi_export]
pub async fn get_message_body(
    mbox: &Mailbox,
    id: Id,
) -> Result<Arc<DecryptedMessage>, ActionError> {
    let ctx = mbox.ctx()?;
    uniffi_async(async move {
        let body = models::Message::message_body(&ctx, id.into()).await?;
        Ok::<_, RealProtonMailError>(Arc::new(DecryptedMessage { ctx, body }))
    })
    .await
    .map_err(ActionError::from)
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
#[uniffi_export]
pub async fn watch_messages_for_label(
    session: Arc<MailUserSession>,
    label_id: Id,
    callback: Box<dyn LiveQueryCallback>,
) -> Result<WatchedMessages, ActionError> {
    let user_context = session.ctx()?;
    let stash = session.user_stash()?;
    uniffi_async(async move {
        let tether = stash.connection();
        let messages = RealMessage::in_label(label_id.into(), &tether).await?;
        let handle = RealMessage::watch(&stash)?;
        let watcher = watch_channel(user_context, handle, callback);
        Result::<_, RealProtonMailError>::Ok(WatchedMessages {
            messages: messages.map_vec(),
            handle: watcher,
        })
    })
    .await
    .map_err(ActionError::from)
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
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn apply_label_to_messages(
    session: Arc<MailUserSession>,
    label_id: Id,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        RealMessage::action_apply_label(
            user_context.action_queue(),
            label_id.into(),
            message_ids.map_vec(),
        )
        .await
        .map(|_| ())
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
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
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn star_messages(
    session: Arc<MailUserSession>,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        RealMessage::action_star(user_context.action_queue(), message_ids.map_vec())
            .await
            .map(|_| ())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
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
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn unstar_messages(
    session: Arc<MailUserSession>,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        RealMessage::action_unstar(user_context.action_queue(), message_ids.map_vec())
            .await
            .map(|_| ())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
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
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn remove_label_from_messages(
    session: Arc<MailUserSession>,
    label_id: Id,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = session.ctx()?;
    uniffi_async(async move {
        RealMessage::action_remove_label(
            user_context.action_queue(),
            label_id.into(),
            message_ids.map_vec(),
        )
        .await
        .map(|_| ())
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
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
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn mark_messages_read(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = mailbox.ctx()?;
    let label_id = mailbox.label_id();
    uniffi_async(async move {
        RealMessage::action_mark_read(
            user_context.action_queue(),
            label_id.into(),
            message_ids.map_vec(),
        )
        .await
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
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
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn mark_messages_unread(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = mailbox.ctx()?;
    let label_id = mailbox.label_id();
    uniffi_async(async move {
        RealMessage::action_mark_unread(
            user_context.action_queue(),
            label_id.into(),
            message_ids.map_vec(),
        )
        .await
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
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
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn delete_messages(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = mailbox.ctx()?;
    let label_id = mailbox.label_id();
    uniffi_async(async move {
        RealMessage::action_delete(
            user_context.action_queue(),
            label_id.into(),
            message_ids.map_vec(),
        )
        .await
        .map(|_| ())
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// Mark multiple messages as ham (not spam) AKA as legitimate
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
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn mark_messages_ham(mailbox: Arc<Mailbox>, message_id: Id) -> Result<(), ActionError> {
    let ctx = mailbox.ctx()?;
    uniffi_async(async move {
        RealMessage::action_ham(ctx.action_queue(), vec![message_id.into()])
            .await
            .map(|_| ())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// Blocks an address.
///
/// # Parameters
///
/// * `session`    - The session to use for the request.
/// * `address_id` - The id of the address to block.
///
/// # Errors
///
/// Returns an error if the action can not be executed.
///
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn block_address(
    session: Arc<MailUserSession>,
    address_id: Id,
) -> Result<(), ActionError> {
    let ctx = session.ctx()?;
    uniffi_async(async move {
        IncomingDefaultLocation::action_block(ctx.action_queue(), address_id.into())
            .await
            .map(|_| ())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// Unblocks an address.
/// This should not be used on addresses that aren't blocked.
///
/// # Parameters
///
/// * `mailbox`    - The session to use for the request.
/// * `address_id` - The id of the address to block.
///
/// # Errors
///
/// Returns an error if the action can not be executed.
///
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn unblock_address(mailbox: Arc<Mailbox>, address_id: Id) -> Result<(), ActionError> {
    let ctx = mailbox.ctx()?;
    uniffi_async(async move {
        IncomingDefaultLocation::action_unblock(ctx.action_queue(), address_id.into())
            .await
            .map(|_| ())
            .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// Mark multiple messages as ham (not spam) AKA as legitimate
///
/// # Parameters
///
/// * `session`  - The session to use for the request.
/// * `id`       - The local ID of the message to mark as spam
///
/// # Errors
///
/// This will always error out as it's not yet implemented
///
#[allow(unused)]
#[allow(clippy::unused_async)]
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn phising_attempt(
    mailbox: Arc<MailUserSession>,
    message_id: Id,
) -> Result<(), ActionError> {
    Err(ActionError::Other(ProtonError::OtherReason(
        crate::errors::OtherErrorReason::Other("Unimplemented!".into()),
    )))
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
#[uniffi_export]
pub async fn label_messages_as(
    mailbox: Arc<Mailbox>,
    message_ids: Vec<Id>,
    selected_label_ids: Vec<Id>,
    partially_selected_label_ids: Vec<Id>,
    must_archive: bool,
) -> Result<bool, ActionError> {
    let user_context = mailbox.ctx()?;
    let source_label_id = mailbox.label_id();
    uniffi_async(async move {
        Result::<_, RealProtonMailError>::Ok(
            RealMessage::action_label_as(
                user_context.action_queue(),
                source_label_id.into(),
                message_ids.into_iter().map_into().collect(),
                selected_label_ids.into_iter().map_into().collect(),
                partially_selected_label_ids
                    .into_iter()
                    .map_into()
                    .collect(),
                must_archive,
            )
            .await?,
        )
    })
    .await
    .map_err(ActionError::from)
}

/// Move given messages from a label into another.
///
/// # Parameters
///
/// * `mailbox`        - Mailbox containing the messages.
/// * `destination_id` - The local ID of the destination label.
/// * `message_ids`    - The local IDs of the messages to move.
///
/// # Errors
///
/// Returns an error if the action can not be executed.
///
#[uniffi_export]
#[returns(VoidActionResult)]
pub async fn move_messages(
    mailbox: Arc<Mailbox>,
    destination_id: Id,
    message_ids: Vec<Id>,
) -> Result<(), ActionError> {
    let user_context = mailbox.ctx()?;
    let source_id = mailbox.label_id();
    uniffi_async(async move {
        RealMessage::action_move(
            user_context.action_queue(),
            source_id.into(),
            destination_id.into(),
            message_ids.map_vec(),
        )
        .await
        .map(|_| ())
        .map_err(RealProtonMailError::from)
    })
    .await
    .map_err(ActionError::from)
    .into()
}

/// [`RemoteId`] on its own is useless, because all our UniFFI endpoints operate on
/// local ids. This method translates remote id into local [`Id`].
///
/// It may happen, that the [`RemoteId`] points to the message that does not exist in our
/// database yet. In that case, Rust SDK will fetch necessary information from API before returning the id.
///
/// # Errors
///
/// Returns an error if the network failed or if the database cannot write/read message.
///
#[uniffi_export]
pub async fn resolve_message_id(
    session: Arc<MailUserSession>,
    remote_id: RemoteId,
) -> Result<Id, ActionError> {
    let user_ctx = session.ctx()?;
    uniffi_async(async move {
        let local_id = RealMessage::find_or_fetch_by_remote_id(&user_ctx, remote_id.into()).await?;
        Ok::<_, RealProtonMailError>(local_id.into())
    })
    .await
    .map_err(ActionError::from)
    .into()
}
