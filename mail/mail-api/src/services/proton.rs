#![allow(async_fn_in_trait)]

//! The Proton API service.
//!
//! This module provides a service that can be used to make requests to the
//! Proton API. Each method provided should match 1:1 with an API endpoint, and
//! follow the naming convention of the endpoint. For example, the endpoint
//! `GET /contacts` should have a method provided called `get_contacts()`.
//!
//! Note that this module extends the code Proton API service with additional
//! functionality relating to mail.
//!
//! For full documentation on the core API implementation, see [`Proton`](proton_api_core::services::proton::Proton).
//!

use bytes::Bytes;
use proton_api_core::service::ApiServiceResult;
use proton_api_core::services::proton::{IncomingDefaultId, LabelId};
use std::time::Duration;

use crate::services::proton::prelude::*;

pub mod common;
pub mod prelude;
pub mod request_data;
pub mod requests;
pub mod response_data;
pub mod responses;

mod proton_impl;

/// The Proton Mail API base path (v4).
pub const MAIL_V4: &str = "/mail/v4";

pub trait ProtonMail {
    /// GETs a single attachment.
    ///
    /// Calls the API to load encrypted attachment content for the given
    /// attachment.
    ///
    /// This returns the full attachment.
    ///
    /// For more details see [the API documentation](https://protonmail.gitlab-pages.protontech.ch/Slim-API/mail/#tag/Attachment).
    ///
    /// # Parameters
    ///
    /// * `attachment_id` - The ID of the attachment to get.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_attachment(&self, attachment_id: AttachmentId) -> ApiServiceResult<Bytes>;

    /// GETs metadata for an attachment.
    ///
    /// Calls the API to load the full attachment metadata for decrypting its
    /// content.
    ///
    /// For more details see [the API documentation](https://protonmail.gitlab-pages.protontech.ch/Slim-API/mail/#tag/Attachment).
    ///
    /// # Parameters
    ///
    /// * `attachment_id` - The ID of the attachment to get metadata for.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_attachment_metadata(
        &self,
        attachment_id: AttachmentId,
    ) -> ApiServiceResult<GetAttachmentMetadataResponse>;

    /// GETs incoming defaults
    ///
    /// Calls the API to get a page of the incoming defaults.
    ///
    /// For more details see [the API documentation](https://protonmail.gitlab-pages.protontech.ch/Slim-API/mail/#tag/IncomingDefaults/operation/get_mail-%7B_version%7D-incomingdefaults).
    async fn get_incoming_defaults(
        &self,
        page: u64,
    ) -> ApiServiceResult<GetIncomingDefaultResponse>;

    /// POSTs incoming default, creates a new one.
    ///
    /// Calls the API to get a page of the incoming defaults.
    ///
    /// For more details see [the API documentation](https://protonmail.gitlab-pages.protontech.ch/Slim-API/mail/#tag/IncomingDefaults/operation/post_mail-%7B_version%7D-incomingdefaults).
    async fn post_incoming_default(
        &self,
        location: IncomingDefaultLocation,
        email: &str,
    ) -> ApiServiceResult<PostIncomingDefaultResponse>;

    /// PUTs incoming defaults, updating it.
    ///
    /// Calls the API to get a page of the incoming defaults.
    ///
    /// For more details see [the API documentation](https://protonmail.gitlab-pages.protontech.ch/Slim-API/mail/#tag/IncomingDefaults/operation/put_mail-%7B_version%7D-incomingdefaults-%7Bid%7D)
    async fn delete_incoming_default(&self, id: &IncomingDefaultId) -> ApiServiceResult<()>;

    /// Upload attachment data with the given `params`.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    async fn post_attachment(
        &self,
        params: NewAttachmentParams,
    ) -> ApiServiceResult<PostAttachmentResponse>;

    /// Delete an attachment with `id` on the server.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    async fn delete_attachment(&self, id: AttachmentId) -> ApiServiceResult<()>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `conversation_id` - The ID of the conversation to get
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_conversation(
        &self,
        conversation_id: ConversationId,
    ) -> ApiServiceResult<GetConversationResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `options` - The options to use for the request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_conversations(
        &self,
        options: GetConversationsOptions,
    ) -> ApiServiceResult<GetConversationsResponse>;

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_conversations_count(&self) -> ApiServiceResult<GetConversationsCountResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `message_id` - The ID of the message to get
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_message(&self, message_id: MessageId) -> ApiServiceResult<GetMessageResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `options` - The options to use for the request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_messages(
        &self,
        options: GetMessagesOptions,
    ) -> ApiServiceResult<GetMessagesResponse>;

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_messages_count(&self) -> ApiServiceResult<GetMessagesCountResponse>;

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_mail_settings(&self) -> ApiServiceResult<GetMailSettingsResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `conversation_ids` - TODO: Document this parameter.
    /// * `label_id`         - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_conversations_delete(
        &self,
        conversation_ids: Vec<ConversationId>,
        label_id: LabelId,
    ) -> ApiServiceResult<PutConversationsDeleteResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `conversation_ids` - TODO: Document this parameter.
    /// * `label_id`         - TODO: Document this parameter.
    /// * `spam_action`      - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_conversations_label(
        &self,
        conversation_ids: Vec<ConversationId>,
        label_id: LabelId,
        spam_action: Option<bool>,
    ) -> ApiServiceResult<PutConversationsLabelResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `conversation_ids` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_conversations_read(
        &self,
        conversation_ids: Vec<ConversationId>,
    ) -> ApiServiceResult<PutConversationsReadResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `conversation_ids` - TODO: Document this parameter.
    /// * `label_id`         - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_conversations_unlabel(
        &self,
        conversation_ids: Vec<ConversationId>,
        label_id: LabelId,
    ) -> ApiServiceResult<PutConversationsUnlabelResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `conversation_ids` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_conversations_unread(
        &self,
        conversation_ids: Vec<ConversationId>,
    ) -> ApiServiceResult<PutConversationsUnreadResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `message_ids` - TODO: Document this parameter.
    /// * `label_id`    - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_messages_delete(
        &self,
        message_ids: Vec<MessageId>,
        label_id: Option<LabelId>,
    ) -> ApiServiceResult<PutMessagesDeleteResponse>;

    /// Put a label on some messages.
    ///
    /// # Parameters
    ///
    /// * `message_ids` - Ids of the messages.
    /// * `label_id`    - Id of the label to set.
    /// * `spam_action` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_messages_label(
        &self,
        message_ids: Vec<MessageId>,
        label_id: LabelId,
        spam_action: Option<bool>,
    ) -> ApiServiceResult<PutMessagesLabelResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `message_ids` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_messages_read(
        &self,
        message_ids: Vec<MessageId>,
    ) -> ApiServiceResult<PutMessagesReadResponse>;

    /// Remove a label from some messages.
    ///
    /// # Parameters
    ///
    /// * `message_ids` - Ids of the messages.
    /// * `label_id`    - Id of the label to remove.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_messages_unlabel(
        &self,
        message_ids: Vec<MessageId>,
        label_id: LabelId,
    ) -> ApiServiceResult<PutMessagesUnlabelResponse>;

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `message_ids` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_messages_unread(
        &self,
        message_ids: Vec<MessageId>,
    ) -> ApiServiceResult<PutMessagesUnreadResponse>;

    /// Mark message as not spam (ham)
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_message_ham(&self, id: &MessageId) -> ApiServiceResult<PutMessageHamResponse>;

    /// Relabel a message.
    ///
    /// Set the message to have the labels passed in the request. The labels are added and removed as necessary.
    /// If either INBOX, SENT or DRAFT are supposed to be added. The correct one according to the message flags will be added.
    /// Note that a maximum of 150 labels IDs can be passed by request.
    ///
    /// # Parameters
    ///
    /// * `message_id` - Id of the message to relabel.
    /// * `label_ids`  - List of labels that must be set.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn relabel_message(
        &self,
        message_id: MessageId,
        label_ids: Vec<LabelId>,
    ) -> ApiServiceResult<PostMessagesRelabelResponse>;

    /// This method creates a new draft message on the server.
    ///
    /// # Params
    ///
    ///  * `message`                 - Draft message details
    ///  * `attachments`             - Map of attachment id to attachment to base64 encoded
    ///    key packet.
    ///  * `reply_or_forward_params` - Required parameters when replying of forwarding a message.
    ///
    /// # Errors
    ///
    /// Returns error if the request fails.
    async fn create_draft(
        &self,
        message: DraftParams,
        attachments: DraftAttachmentKeyPackets,
        reply_or_forward_params: Option<DraftReplyOrForwardParams>,
    ) -> ApiServiceResult<PostCreateDraftResponse>;

    /// This method will update a draft message on the server.
    ///
    /// # Params
    ///
    ///  * `message_id`  - message id to update
    ///  * `message`     - Draft message details
    ///  * `attachments` - Map of attachment id to attachment to base64 encoded
    ///    key packet.
    ///
    /// # Errors
    ///
    /// Returns error if the request fails.
    async fn update_draft(
        &self,
        message_id: MessageId,
        message: DraftParams,
        attachments: DraftAttachmentKeyPackets,
    ) -> ApiServiceResult<PutUpdateDraftResponse>;

    /// Sends an e-mail send request to the server.
    ///
    /// # Params
    ///
    ///  * `message_id`         - message id (draft) to send.
    ///  * `packages`           - The packages of the message containing the encrypted e-mail data for the recipients.
    ///  * `auto_save_contacts` - Whether the server should automatically create contacts for the recipients.
    ///  * `delay`              - Duration by which the message should be delayed before sending
    ///
    /// # Errors
    ///
    /// Returns error if the request fails.
    async fn send_mail(
        &self,
        message_id: MessageId,
        packages: Vec<Package>,
        auto_save_contacts: Option<bool>,
        delay: Option<Duration>,
    ) -> ApiServiceResult<PostSendMessageResponse>;

    /// Reports a message as phishing.
    /// It requires the decrypted message body.
    async fn report_phishing(
        &self,
        message_id: MessageId,
        mime_type: MimeType,
        body: &str,
    ) -> ApiServiceResult<()>;

    /// Cancel the sending of a message with `message_id`, which was previously sent.
    ///
    /// # Errors
    ///
    /// Returns error if the request fails
    async fn cancel_send(&self, message_id: MessageId) -> ApiServiceResult<PostCancelSendResponse>;

    /// Delete all messages with a label/folder
    ///
    /// # Errors
    ///
    /// Returns error if the request fails
    async fn delete_all_messages_in_label(&self, label_id: LabelId) -> ApiServiceResult<()>;
}
