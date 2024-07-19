use crate::mail::mailbox::DEFAULT_CONVERSATION_COUNT;
use crate::mail::mailbox::{Observable, SharedLive, SharedLiveQueryBuilder};
use crate::mail::settings::MailUserSettings;
use crate::mail::{Mailbox, MailboxError, MailboxLiveQueryUpdatedCallback};
use crate::new_live_query;
use proton_mail_common::db::proton_sqlite3::InProcessTrackerService;
use proton_mail_common::db::{LocalMessageId, MessageQuery};
use proton_mail_common::exports::parking_lot::RwLock;
use proton_mail_common::exports::{proton_mail_html_transformer, thiserror};
use proton_mail_common::proton_api_mail::domain::MimeType;
use proton_mail_common::{MailboxObservableQueryBuilder, ParsedHeaderValue};
use std::sync::Arc;

#[uniffi::export]
impl Mailbox {
    /// Create a live query for messages for the currently selected label.
    ///
    /// # Errors
    /// Return error if the mailbox's view mode is not [`MailSettingsViewMode::Messages`].
    pub fn new_message_live_query(
        &self,
        limit: i64,
        cb: Box<dyn MailboxLiveQueryUpdatedCallback>,
    ) -> Result<Arc<MailboxMessageLiveQuery>, MailboxError> {
        let limit = usize::try_from(limit).unwrap_or(DEFAULT_CONVERSATION_COUNT);
        let builder = FFIObservableMessagesQueryBuilder(cb);
        Ok(self.mbox.new_messages_query(builder, limit)?)
    }

    /// Retrieve and decrypt the body of message with `id`.
    ///
    /// If the message body has never been fetched before, it will be retrieved from the
    /// servers.
    ///
    /// # Errors
    /// Returns error if the network request, the database query, reading/writing
    /// the body to the cache or decrypting the body failed.
    pub async fn message_body(
        &self,
        id: u64,
        mail_settings: &MailUserSettings,
    ) -> Result<DecryptedMessage, MailboxError> {
        let settings = mail_settings.value().unwrap_or_default();
        let mbox = self.mbox.clone();
        self.uniffi_async(async move {
            Ok(DecryptedMessage {
                message: RwLock::new(
                    mbox.message_body(LocalMessageId::from(id), &settings)
                        .await?,
                ),
            })
        })
        .await
    }
}

/// Contains the decrypted and parsed message body from a message.
#[derive(uniffi::Object)]
pub struct DecryptedMessage {
    message: RwLock<proton_mail_common::DecryptedMessage>,
}

/// # Safety
/// The `NodeRef` type is not Send since it uses Rc, however the only way to access it is via the
/// `RwLock` which itself will be wrapped by an `Arc`. No `Rc` references are shared outside of this
/// object.
unsafe impl Send for DecryptedMessage {}
/// # Safety
/// The `NodeRef` type is not Send since it uses Rc, however the only way to access it is via the
/// `RwLock` which itself will be wrapped by an `Arc`. No `Rc` references are shared outside of this
/// object.
unsafe impl Sync for DecryptedMessage {}

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum DecryptedMessageError {
    #[error("Body type is not valid for this operation")]
    InvalidBodyType,
    #[error("Html Tansformer: {0}")]
    Transform(proton_mail_html_transformer::Error),
}

impl From<proton_mail_common::DecryptedMessageError> for DecryptedMessageError {
    fn from(value: proton_mail_common::DecryptedMessageError) -> Self {
        match value {
            proton_mail_common::DecryptedMessageError::InvalidBodyType => Self::InvalidBodyType,
            proton_mail_common::DecryptedMessageError::Transform(e) => Self::Transform(e),
        }
    }
}

#[uniffi::export]
impl DecryptedMessage {
    /// The message id of which this body belongs to.
    pub fn id(&self) -> u64 {
        self.message.read().metadata().id.value()
    }

    /// The mime type of the message.
    pub fn mime_type(&self) -> MimeType {
        self.message.read().metadata().mime_type
    }
    /// Returns the decrypted body of the message.
    pub fn body(&self) -> String {
        self.message.read().body().to_owned()
    }

    /// Returns the header strings associated with the message.
    pub fn header_string(&self) -> String {
        self.message.read().metadata().header.clone()
    }

    /// Retrieve a parsed header value for a given `key`.
    pub fn parsed_header_value(&self, key: &str) -> Option<ParsedHeaderValue> {
        self.message.read().parsed_header_value(key)
    }

    /// Disable remote images.
    ///
    /// # Errors
    ///
    /// Returns error if the process failed.
    pub fn disable_remote_images(&self) -> Result<(), DecryptedMessageError> {
        Ok(self.message.write().disable_remote_images()?)
    }

    /// Enable remote images.
    ///
    /// # Errors
    ///
    /// Returns error if the process failed.
    pub fn enable_remote_images(&self) -> Result<(), DecryptedMessageError> {
        Ok(self.message.write().enable_remote_images()?)
    }
}

new_live_query!(MailboxMessageLiveQuery, MessageQuery);

struct FFIObservableMessagesQueryBuilder(Box<dyn MailboxLiveQueryUpdatedCallback>);
impl MailboxObservableQueryBuilder<MessageQuery> for FFIObservableMessagesQueryBuilder {
    type Output = Arc<MailboxMessageLiveQuery>;

    fn build(self, tracker: InProcessTrackerService, query: MessageQuery) -> Self::Output {
        MailboxMessageLiveQuery::new(tracker, query, self.0)
    }
}
