//! Functions for working with [`Message`]s.
//!
//! The functions presented here can operate in one of two scopes: either on a
//! [`Mailbox`], or on a [`MailSession`]. The difference is that operations that
//! rely on the context of a mailbox/label view are performed on a mailbox, and
//! operations that are more global in nature are performed on a session. The
//! scope of the methods might change over time, but their primary association
//! of working with messages, and hence their placement in this module, won't.
//!

use crate::mail::datatypes::{Message, MessageSearchOptions};
use crate::mail::{MailSession, MailSessionError, MailboxError};
use itertools::Itertools as _;
use proton_core_common::datatypes::LocalId;
use proton_mail_common::decrypted_message::{self, DecryptedMessageBody};
use proton_mail_common::models::{MailSettings, Message as RealMessage};
use proton_mail_common::MailUserContext;
use stash::orm::Model as _;
use std::sync::Arc;

use super::datatypes::MessageAvailableAction;
use super::datatypes::{BlockQuote, RemoteContent};
use super::{Mailbox, MailboxResult};

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
pub struct BodyOutput {
    /// The transformed html of the message.
    body: String,
}

#[uniffi::export]
impl DecryptedMessage {
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
        let user_session_id = user_ctx.user_id();
        let mail_settings = MailSettings::get(&user_ctx.stash().into())
            .await
            .unwrap_or_default()
            .unwrap_or_default();

        let body = decrypted_message::transform_html(
            &self.body.body,
            opts.remote_content.into(),
            opts.block_quote.into(),
            &mail_settings,
            user_session_id,
        );
        Ok(BodyOutput { body })
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
    session: Arc<MailSession>,
    options: MessageSearchOptions,
) -> Result<Vec<Message>, MailSessionError> {
    Ok(
        RealMessage::search(options.into(), session.api(), session.stash())
            .await?
            .into_iter()
            .map(Into::into)
            .collect(),
    )
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
    session: Arc<MailSession>,
    id: u64,
) -> MailboxResult<Vec<MessageAvailableAction>> {
    let Some(message) = RealMessage::load(id.into(), session.stash()).await? else {
        return Ok(vec![]);
    };
    Ok(message
        .available_actions(session.stash())
        .await?
        .into_iter()
        .map_into()
        .collect())
}
/// Return the decrypted body of the specified message.
///
/// If the message body has never been fetched before, it will be retrieved from
/// the servers.
/// Obtains a [`DecryptedMessage`] given a message id.
#[uniffi::export]
pub async fn get_message_body(mbox: &Mailbox, id: u64) -> MailboxResult<DecryptedMessage> {
    Ok(DecryptedMessage {
        ctx: mbox.mbox().user_context(),
        body: mbox.mbox().message_body(LocalId(id)).await?,
    })
}
