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

pub mod common;
pub mod request_data;
pub mod requests;
pub mod response_data;
pub mod responses;

use crate::services::proton::common::LabelType;
use crate::services::proton::requests::{
    GetConversationsOptions, GetLabelsOptions, GetMessagesOptions, PatchLabelRequest,
    PostLabelsRequest, PutConversationsDeleteRequest, PutConversationsLabelRequest,
    PutConversationsReadRequest, PutConversationsUnlabelRequest, PutConversationsUnreadRequest,
    PutLabelRequest, PutMessagesDeleteRequest, PutMessagesLabelRequest, PutMessagesReadRequest,
    PutMessagesUnlabelRequest, PutMessagesUnreadRequest,
};
use crate::services::proton::responses::{
    GetAttachmentMetadataResponse, GetConversationResponse, GetConversationsCountResponse,
    GetConversationsResponse, GetLabelsResponse, GetMessageResponse, GetMessagesCountResponse,
    GetMessagesResponse, GetSettingsResponse, PatchLabelResponse, PostLabelsResponse,
    PutConversationsDeleteResponse, PutConversationsLabelResponse, PutConversationsReadResponse,
    PutConversationsUnlabelResponse, PutConversationsUnreadResponse, PutLabelResponse,
    PutMessagesDeleteResponse, PutMessagesLabelResponse, PutMessagesReadResponse,
    PutMessagesUnlabelResponse, PutMessagesUnreadResponse,
};
use crate::{MAX_LIMIT_VALUE_U64, MAX_PAGE_ELEMENT_COUNT_U64};
use bytes::Bytes;
use proton_api_core::service::{ApiService, ApiServiceError, Json, NO_PARAMS};
use proton_api_core::services::proton::common::RemoteId;
use proton_api_core::services::proton::Proton;
use velcro::hash_map;

pub trait ProtonMail: ApiService {
    const BASE_PATH_CORE: &'static str = "core/v4";
    const BASE_PATH_MAIL: &'static str = "mail/v4";

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `label_id` - The ID of the label to delete.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn delete_label(&self, label_id: RemoteId) -> Result<(), ApiServiceError> {
        self.delete::<_, ()>(
            &format!("{}/labels/{label_id}", Self::BASE_PATH_CORE),
            NO_PARAMS,
            None,
        )
        .await
    }

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
    async fn get_attachment(&self, attachment_id: RemoteId) -> Result<Bytes, ApiServiceError> {
        self.get::<_, Bytes>(
            &format!("{}/attachments/{attachment_id}", Self::BASE_PATH_MAIL),
            NO_PARAMS,
            None,
        )
        .await
    }

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
        attachment_id: RemoteId,
    ) -> Result<GetAttachmentMetadataResponse, ApiServiceError> {
        self.get::<_, Json<_>>(
            &format!(
                "{}/attachments/{attachment_id}/metadata",
                Self::BASE_PATH_MAIL
            ),
            NO_PARAMS,
            None,
        )
        .await
    }

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
        conversation_id: RemoteId,
    ) -> Result<GetConversationResponse, ApiServiceError> {
        self.get::<_, Json<_>>(
            &format!("{}/conversations/{conversation_id}", Self::BASE_PATH_MAIL),
            NO_PARAMS,
            None,
        )
        .await
    }

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
        mut options: GetConversationsOptions,
    ) -> Result<GetConversationsResponse, ApiServiceError> {
        options.page_size = options.page_size.max(MAX_PAGE_ELEMENT_COUNT_U64);
        options.limit = options.limit.map(|v| v.max(MAX_LIMIT_VALUE_U64));
        self.get::<_, Json<_>>(
            &format!("{}/conversations", Self::BASE_PATH_MAIL),
            Some(options),
            None,
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_conversations_count(
        &self,
    ) -> Result<GetConversationsCountResponse, ApiServiceError> {
        self.get::<_, Json<_>>(
            &format!("{}/conversations/count", Self::BASE_PATH_MAIL),
            NO_PARAMS,
            None,
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `label_type` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_labels(
        &self,
        label_type: LabelType,
    ) -> Result<GetLabelsResponse, ApiServiceError> {
        self.get::<_, Json<_>>(
            &format!("{}/labels", Self::BASE_PATH_CORE),
            Some(GetLabelsOptions { label_type }),
            None,
        )
        .await
    }

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
    async fn get_message(
        &self,
        message_id: RemoteId,
    ) -> Result<GetMessageResponse, ApiServiceError> {
        self.get::<_, Json<_>>(
            &format!("{}/messages/{message_id}", Self::BASE_PATH_MAIL),
            NO_PARAMS,
            None,
        )
        .await
    }

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
        mut options: GetMessagesOptions,
    ) -> Result<GetMessagesResponse, ApiServiceError> {
        options.page_size = options.page_size.max(MAX_PAGE_ELEMENT_COUNT_U64);
        options.limit = options.limit.map(|v| v.max(MAX_LIMIT_VALUE_U64));
        // TODO: Document why we send as POST and override the method. For privacy?
        self.post::<_, Json<_>>(
            &format!("{}/messages", Self::BASE_PATH_MAIL),
            Some(options),
            Some(hash_map! {
                "X-HTTP-Method-Override".to_owned(): "GET".to_owned(),
            }),
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_messages_count(&self) -> Result<GetMessagesCountResponse, ApiServiceError> {
        self.get::<_, Json<_>>(
            &format!("{}/messages/count", Self::BASE_PATH_MAIL),
            NO_PARAMS,
            None,
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn get_settings(&self) -> Result<GetSettingsResponse, ApiServiceError> {
        self.get::<_, Json<_>>(
            &format!("{}/settings", Self::BASE_PATH_MAIL),
            NO_PARAMS,
            None,
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `body` - The body to use for the request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn post_labels(
        &self,
        body: PostLabelsRequest,
    ) -> Result<PostLabelsResponse, ApiServiceError> {
        self.post::<_, Json<_>>(&format!("{}/labels", Self::BASE_PATH_CORE), body, None)
            .await
    }

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
        conversation_ids: Vec<RemoteId>,
        label_id: RemoteId,
    ) -> Result<PutConversationsDeleteResponse, ApiServiceError> {
        self.put::<_, Json<_>>(
            &format!("{}/conversations/delete", Self::BASE_PATH_MAIL),
            PutConversationsDeleteRequest {
                ids: conversation_ids,
                label_id,
            },
            None,
        )
        .await
    }

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
        conversation_ids: Vec<RemoteId>,
        label_id: RemoteId,
        spam_action: Option<bool>,
    ) -> Result<PutConversationsLabelResponse, ApiServiceError> {
        self.put::<_, Json<_>>(
            &format!("{}/conversations/label", Self::BASE_PATH_MAIL),
            PutConversationsLabelRequest {
                action: 1,
                ids: conversation_ids,
                label_id,
                spam_action,
            },
            None,
        )
        .await
    }

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
        conversation_ids: Vec<RemoteId>,
    ) -> Result<PutConversationsReadResponse, ApiServiceError> {
        self.put::<_, Json<_>>(
            &format!("{}/conversations/read", Self::BASE_PATH_MAIL),
            PutConversationsReadRequest {
                ids: conversation_ids,
            },
            None,
        )
        .await
    }

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
        conversation_ids: Vec<RemoteId>,
        label_id: RemoteId,
    ) -> Result<PutConversationsUnlabelResponse, ApiServiceError> {
        self.put::<_, Json<_>>(
            &format!("{}/conversations/unlabel", Self::BASE_PATH_MAIL),
            PutConversationsUnlabelRequest {
                ids: conversation_ids,
                label_id,
            },
            None,
        )
        .await
    }

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
        conversation_ids: Vec<RemoteId>,
    ) -> Result<PutConversationsUnreadResponse, ApiServiceError> {
        self.put::<_, Json<_>>(
            &format!("{}/conversations/unread", Self::BASE_PATH_MAIL),
            PutConversationsUnreadRequest {
                ids: conversation_ids,
            },
            None,
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `label_id` - The ID of the label to update.
    /// * `body`     - The body to use for the request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_label(
        &self,
        label_id: RemoteId,
        body: PutLabelRequest,
    ) -> Result<PutLabelResponse, ApiServiceError> {
        self.put::<_, Json<_>>(
            &format!("{}/labels/{label_id}", Self::BASE_PATH_CORE),
            body,
            None,
        )
        .await
    }

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
        message_ids: Vec<RemoteId>,
        label_id: Option<RemoteId>,
    ) -> Result<PutMessagesDeleteResponse, ApiServiceError> {
        self.put::<_, Json<_>>(
            &format!("{}/messages/delete", Self::BASE_PATH_MAIL),
            PutMessagesDeleteRequest {
                ids: message_ids,
                label_id,
            },
            None,
        )
        .await
    }

    /// TODO: Document this method.
    ///
    /// # Parameters
    ///
    /// * `message_ids` - TODO: Document this parameter.
    /// * `label_id`    - TODO: Document this parameter.
    /// * `spam_action` - TODO: Document this parameter.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn put_messages_label(
        &self,
        message_ids: Vec<RemoteId>,
        label_id: RemoteId,
        spam_action: Option<bool>,
    ) -> Result<PutMessagesLabelResponse, ApiServiceError> {
        self.put::<_, Json<_>>(
            &format!("{}/messages/label", Self::BASE_PATH_MAIL),
            PutMessagesLabelRequest {
                action: 1,
                ids: message_ids,
                label_id,
                spam_action,
            },
            None,
        )
        .await
    }

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
        message_ids: Vec<RemoteId>,
    ) -> Result<PutMessagesReadResponse, ApiServiceError> {
        self.put::<_, Json<_>>(
            &format!("{}/messages/read", Self::BASE_PATH_MAIL),
            PutMessagesReadRequest { ids: message_ids },
            None,
        )
        .await
    }

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
    async fn put_messages_unlabel(
        &self,
        message_ids: Vec<RemoteId>,
        label_id: RemoteId,
    ) -> Result<PutMessagesUnlabelResponse, ApiServiceError> {
        self.put::<_, Json<_>>(
            &format!("{}/messages/unlabel", Self::BASE_PATH_MAIL),
            PutMessagesUnlabelRequest {
                ids: message_ids,
                label_id,
            },
            None,
        )
        .await
    }

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
        message_ids: Vec<RemoteId>,
    ) -> Result<PutMessagesUnreadResponse, ApiServiceError> {
        self.put::<_, Json<_>>(
            &format!("{}/messages/unread", Self::BASE_PATH_MAIL),
            PutMessagesUnreadRequest { ids: message_ids },
            None,
        )
        .await
    }

    /// This method is used to patch an existing label.
    /// The `label_id` is used to identify the label to patch.
    /// Body contains expanded and notify fields.
    /// Expanded is a boolean that indicates if the label is expanded.
    /// For example if the folder is expanded in the UI.
    /// Notify is a boolean that indicates if the user should be notified
    /// about new messages in the label. By default both of them are disabled.
    ///
    /// # Parameters
    ///
    /// * `label_id` - The ID of the label to patch.
    /// * `body` - Json body to use in the patch request.
    ///
    /// # Errors
    ///
    /// This method will return an error if the request fails.
    ///
    async fn patch_label(
        &self,
        label_id: RemoteId,
        body: PatchLabelRequest,
    ) -> Result<PatchLabelResponse, ApiServiceError> {
        self.patch::<_, Json<_>>(
            &format!("{}/labels/{label_id}", Self::BASE_PATH_CORE),
            body,
            None,
        )
        .await
    }
}

impl ProtonMail for Proton {}
