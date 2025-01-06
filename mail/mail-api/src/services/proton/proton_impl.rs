use bytes::Bytes;

use proton_api_core::service::{ApiServiceError, ApiServiceResult};
use proton_api_core::services::proton::common::{LabelId, RemoteId};
use proton_api_core::services::proton::muon::serde_to_query;
use proton_api_core::services::proton::muon::util::ProtonRequestExt;
use proton_api_core::services::proton::muon::{DELETE, GET, PATCH, POST, PUT};
use proton_api_core::services::proton::Proton;
use proton_api_core::services::proton::CORE_V4;

use crate::services::proton::prelude::*;
use crate::services::proton::{Package, PostSendRequest, MAIL_V4};
use crate::services::proton::{PostSendMessageResponse, ProtonMail};
use crate::{MAX_LIMIT_VALUE_U64, MAX_PAGE_ELEMENT_COUNT_U64};

impl ProtonMail for Proton {
    async fn delete_label(&self, label_id: RemoteId) -> ApiServiceResult<()> {
        DELETE!("{CORE_V4}/labels/{label_id}")
            .send_with(self)
            .await?
            .ok()?;

        Ok(())
    }

    async fn get_attachment(&self, attachment_id: AttachmentId) -> ApiServiceResult<Bytes> {
        Ok(GET!("{MAIL_V4}/attachments/{attachment_id}")
            .send_with(self)
            .await?
            .ok()?
            .into_body()
            .into())
    }

    async fn get_attachment_metadata(
        &self,
        attachment_id: AttachmentId,
    ) -> ApiServiceResult<GetAttachmentMetadataResponse> {
        Ok(GET!("{MAIL_V4}/attachments/{attachment_id}/metadata")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_conversation(
        &self,
        conversation_id: RemoteId,
    ) -> ApiServiceResult<GetConversationResponse> {
        Ok(GET!("{MAIL_V4}/conversations/{conversation_id}")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_conversations(
        &self,
        mut options: GetConversationsOptions,
    ) -> ApiServiceResult<GetConversationsResponse> {
        options.page_size = options.page_size.min(MAX_PAGE_ELEMENT_COUNT_U64);
        options.limit = options.limit.map(|v| v.min(MAX_LIMIT_VALUE_U64));

        Ok(GET!("{MAIL_V4}/conversations")
            .query(serde_to_query(options)?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_conversations_count(&self) -> ApiServiceResult<GetConversationsCountResponse> {
        Ok(GET!("{MAIL_V4}/conversations/count")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_labels(&self, label_type: LabelType) -> ApiServiceResult<GetLabelsResponse> {
        Ok(GET!("{CORE_V4}/labels")
            .query(serde_to_query(GetLabelsOptions { label_type })?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_labels_by_ids(
        &self,
        label_ids: Vec<LabelId>,
    ) -> ApiServiceResult<GetLabelsResponse> {
        Ok(POST!("{CORE_V4}/labels/by-ids")
            .body_json(GetLabelsByIdsOptions { label_ids })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_message(&self, message_id: RemoteId) -> ApiServiceResult<GetMessageResponse> {
        Ok(GET!("{MAIL_V4}/messages/{message_id}")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_messages(
        &self,
        mut options: GetMessagesOptions,
    ) -> ApiServiceResult<GetMessagesResponse> {
        options.page_size = options.page_size.min(MAX_PAGE_ELEMENT_COUNT_U64);
        options.limit = options.limit.map(|v| v.min(MAX_LIMIT_VALUE_U64));

        // There can potentially be a large number of query parameters in this request.
        // The length of the URL could eventually exceed the limit imposed by our API.
        // To avoid this, we can send as POST, with the query parameters sent in the message body.
        // In this case, add this as a header: `X-HTTP-Method-Override`: `GET`
        // Mobile needs this to be `get` so they can run their tests through the proxy.
        Ok(GET!("{MAIL_V4}/messages")
            .query(serde_to_query(options)?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_messages_count(&self) -> ApiServiceResult<GetMessagesCountResponse> {
        Ok(GET!("{MAIL_V4}/messages/count")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_mail_settings(&self) -> ApiServiceResult<GetMailSettingsResponse> {
        Ok(GET!("{MAIL_V4}/settings")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn post_labels(&self, body: PostLabelsRequest) -> ApiServiceResult<PostLabelsResponse> {
        Ok(POST!("{CORE_V4}/labels")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_conversations_delete(
        &self,
        ids: Vec<RemoteId>,
        label_id: LabelId,
    ) -> ApiServiceResult<PutConversationsDeleteResponse> {
        Ok(PUT!("{MAIL_V4}/conversations/delete")
            .body_json(PutConversationsDeleteRequest { ids, label_id })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_conversations_label(
        &self,
        ids: Vec<RemoteId>,
        label_id: LabelId,
        spam_action: Option<bool>,
    ) -> ApiServiceResult<PutConversationsLabelResponse> {
        Ok(PUT!("{MAIL_V4}/conversations/label")
            .body_json(PutConversationsLabelRequest {
                action: 1,
                ids,
                label_id,
                spam_action,
            })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_conversations_read(
        &self,
        ids: Vec<RemoteId>,
    ) -> ApiServiceResult<PutConversationsReadResponse> {
        Ok(PUT!("{MAIL_V4}/conversations/read")
            .body_json(PutConversationsReadRequest { ids })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_conversations_unlabel(
        &self,
        ids: Vec<RemoteId>,
        label_id: LabelId,
    ) -> ApiServiceResult<PutConversationsUnlabelResponse> {
        Ok(PUT!("{MAIL_V4}/conversations/unlabel")
            .body_json(PutConversationsUnlabelRequest { ids, label_id })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_conversations_unread(
        &self,
        ids: Vec<RemoteId>,
    ) -> ApiServiceResult<PutConversationsUnreadResponse> {
        Ok(PUT!("{MAIL_V4}/conversations/unread")
            .body_json(PutConversationsUnreadRequest { ids })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_label(
        &self,
        label_id: LabelId,
        body: PutLabelRequest,
    ) -> ApiServiceResult<PutLabelResponse> {
        Ok(PUT!("{CORE_V4}/labels/{label_id}")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_messages_delete(
        &self,
        ids: Vec<RemoteId>,
        label_id: Option<LabelId>,
    ) -> ApiServiceResult<PutMessagesDeleteResponse> {
        Ok(PUT!("{MAIL_V4}/messages/delete")
            .body_json(PutMessagesDeleteRequest { ids, label_id })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_messages_label(
        &self,
        ids: Vec<RemoteId>,
        label_id: LabelId,
        spam_action: Option<bool>,
    ) -> ApiServiceResult<PutMessagesLabelResponse> {
        Ok(PUT!("{MAIL_V4}/messages/label")
            .body_json(PutMessagesLabelRequest {
                action: 1,
                ids,
                label_id,
                spam_action,
            })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_messages_read(
        &self,
        ids: Vec<RemoteId>,
    ) -> ApiServiceResult<PutMessagesReadResponse> {
        Ok(PUT!("{MAIL_V4}/messages/read")
            .body_json(PutMessagesReadRequest { ids })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_messages_unlabel(
        &self,
        ids: Vec<RemoteId>,
        label_id: LabelId,
    ) -> ApiServiceResult<PutMessagesUnlabelResponse> {
        Ok(PUT!("{MAIL_V4}/messages/unlabel")
            .body_json(PutMessagesUnlabelRequest { ids, label_id })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_messages_unread(
        &self,
        ids: Vec<RemoteId>,
    ) -> ApiServiceResult<PutMessagesUnreadResponse> {
        Ok(PUT!("{MAIL_V4}/messages/unread")
            .body_json(PutMessagesUnreadRequest { ids })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn relabel_message(
        &self,
        message_id: RemoteId,
        label_ids: Vec<RemoteId>,
    ) -> ApiServiceResult<PostMessagesRelabelResponse> {
        Ok(POST!("{MAIL_V4}/messages/{message_id}/relabel")
            .body_json(PostMessagesRelabelRequest { label_ids })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn patch_label(
        &self,
        label_id: RemoteId,
        body: PatchLabelRequest,
    ) -> ApiServiceResult<PatchLabelResponse> {
        Ok(PATCH!("{CORE_V4}/labels/{label_id}")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn create_draft(
        &self,
        message: DraftParams,
        action: DraftAction,
        attachment_key_packets: DraftAttachmentKeyPackets,
        parent_id: Option<RemoteId>,
    ) -> ApiServiceResult<PostCreateDraftResponse> {
        Ok(POST!("{MAIL_V4}/messages")
            .body_json(PostCreateDraftRequest {
                message,
                action,
                attachment_key_packets,
                parent_id,
            })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn update_draft(
        &self,
        message_id: RemoteId,
        message: DraftParams,
        attachment_key_packets: DraftAttachmentKeyPackets,
    ) -> ApiServiceResult<PutUpdateDraftResponse> {
        Ok(PUT!("{MAIL_V4}/messages/{message_id}")
            .body_json(PutUpdateDraftRequest {
                message,
                attachment_key_packets,
            })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn send_mail(
        &self,
        message_id: RemoteId,
        packages: Vec<Package>,
        auto_save_contacts: Option<bool>,
    ) -> Result<PostSendMessageResponse, ApiServiceError> {
        let send_request = PostSendRequest {
            expiration_time: None,
            expires_in: None,
            auto_save_contacts,
            delay_seconds: None,
            delivery_time: None,
            packages,
        };

        Ok(POST!("{MAIL_V4}/messages/{message_id}")
            .body_json(send_request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }
}
