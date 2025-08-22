use bytes::Bytes;
use muon::DELETE;
use proton_core_api::service::ApiServiceResult;
use proton_core_api::services::proton::muon::util::ProtonRequestExt;
use proton_core_api::services::proton::muon::{GET, POST, PUT, serde_to_query};
use proton_core_api::services::proton::{CORE_V4, IncomingDefaultId, LabelId, Proton};
use serde_json::json;
use std::io::Cursor;
use std::time::Duration;

use crate::services::proton::prelude::*;
use crate::services::proton::{MAIL_V4, Package, PostSendRequest, UNLEASH_V2};
use crate::services::proton::{PostSendMessageResponse, ProtonMail};
use crate::{INCOMING_DEFAULTS_PAGE_SIZE, MAX_LIMIT_VALUE_U64, MAX_PAGE_ELEMENT_COUNT_U64};

impl ProtonMail for Proton {
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

    async fn get_incoming_defaults(
        &self,
        page: u64,
    ) -> ApiServiceResult<GetIncomingDefaultResponse> {
        let body = json!({
            "Page": page,
            "PageSize": INCOMING_DEFAULTS_PAGE_SIZE,
        });
        Ok(GET!("{MAIL_V4}/incomingdefaults")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn post_incoming_default(
        &self,
        location: IncomingDefaultLocation,
        email: &str,
    ) -> ApiServiceResult<PostIncomingDefaultResponse> {
        let body = json!({
            "Email": email,
            "Location": location,
        });
        Ok(POST!("{MAIL_V4}/incomingdefaults")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn delete_incoming_default(&self, id: &IncomingDefaultId) -> ApiServiceResult<()> {
        let body = json!({
            "IDs": vec![id],
        });
        PUT!("{MAIL_V4}/incomingdefaults/delete")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?;
        Ok(())
    }

    async fn post_attachment(
        &self,
        params: NewAttachmentParams,
    ) -> ApiServiceResult<PostAttachmentResponse> {
        // It is not necessary to have a dedicated request here, but we leave it
        // so that it matches the current setup. Eventually, there may also be
        // an opportunity to directly convert this into form data when muon supports this.
        let request = PostUploadAttachmentRequest::from(params);
        let response = POST!("{MAIL_V4}/attachments")
            .multipart(move |mut form| {
                form.add_text("Filename", request.filename);
                form.add_text("MessageID", request.message_id.into_inner());
                form.add_text("MIMEType", request.mime_type);
                form.add_text(
                    "Disposition",
                    match request.disposition {
                        Disposition::Attachment => "attachment",
                        Disposition::Inline => "inline",
                    },
                );
                if let Some(content_id) = request.content_id {
                    form.add_text("ContentID", content_id);
                }
                //NOTE: Even though this is a sync reader interface, this will only result in mem copies.
                form.add_reader_file("KeyPackets", Cursor::new(request.key_packets), "blob");
                if let Some(signature) = request.signature {
                    form.add_reader_file("Signature", Cursor::new(signature.0), "blob");
                }
                if let Some(enc_signature) = request.enc_signature {
                    form.add_reader_file("EncSignature", Cursor::new(enc_signature.0), "blob");
                }

                //NOTE: Even though this is a sync reader interface, this will only result in mem copies.
                form.add_reader_file("DataPacket", Cursor::new(request.data_packet), "blob");
                form
            })
            .await?
            .send_with(self)
            .await?
            .ok()?;

        Ok(response.into_body_json()?)
    }

    async fn delete_attachment(&self, id: AttachmentId) -> ApiServiceResult<()> {
        DELETE!("{MAIL_V4}/attachments/{id}")
            .send_with(self)
            .await?
            .ok()?;
        Ok(())
    }

    async fn get_conversation(
        &self,
        conversation_id: ConversationId,
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

    async fn get_message(&self, message_id: MessageId) -> ApiServiceResult<GetMessageResponse> {
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

    async fn put_mobile_settings(
        &self,
        mobile_settings: MobileSettings,
    ) -> ApiServiceResult<PutMobileSettingsResponse> {
        Ok(PUT!("{MAIL_V4}/settings/mobilesettings")
            .body_json(mobile_settings)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_conversations_delete(
        &self,
        ids: Vec<ConversationId>,
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
        ids: Vec<ConversationId>,
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
        ids: Vec<ConversationId>,
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
        ids: Vec<ConversationId>,
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
        ids: Vec<ConversationId>,
        label_id: LabelId,
    ) -> ApiServiceResult<PutConversationsUnreadResponse> {
        Ok(PUT!("{MAIL_V4}/conversations/unread")
            .body_json(PutConversationsUnreadRequest { ids, label_id })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_conversations_snooze(
        &self,
        ids: Vec<ConversationId>,
        snooze_time: u64,
    ) -> ApiServiceResult<PutConversationsSnoozeResponse> {
        Ok(PUT!("{MAIL_V4}/conversations/snooze")
            .body_json(PutConversationsSnoozeRequest { ids, snooze_time })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_conversations_unsnooze(
        &self,
        ids: Vec<ConversationId>,
    ) -> ApiServiceResult<PutConversationsUnsnoozeResponse> {
        Ok(PUT!("{MAIL_V4}/conversations/unsnooze")
            .body_json(PutConversationsUnsnoozeRequest { ids })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn put_messages_delete(
        &self,
        ids: Vec<MessageId>,
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
        ids: Vec<MessageId>,
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
        ids: Vec<MessageId>,
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
        ids: Vec<MessageId>,
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
        ids: Vec<MessageId>,
    ) -> ApiServiceResult<PutMessagesUnreadResponse> {
        Ok(PUT!("{MAIL_V4}/messages/unread")
            .body_json(PutMessagesUnreadRequest { ids })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    /// Mark message as not spam
    async fn put_message_ham(&self, id: &MessageId) -> ApiServiceResult<PutMessageHamResponse> {
        Ok(PUT!("{MAIL_V4}/messages/{id}/mark/ham")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn relabel_message(
        &self,
        message_id: MessageId,
        label_ids: Vec<LabelId>,
    ) -> ApiServiceResult<PostMessagesRelabelResponse> {
        Ok(POST!("{MAIL_V4}/messages/{message_id}/relabel")
            .body_json(PostMessagesRelabelRequest { label_ids })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn create_draft(
        &self,
        message: DraftParams,
        attachment_key_packets: DraftAttachmentKeyPackets,
        reply_or_forward_params: Option<DraftReplyOrForwardParams>,
    ) -> ApiServiceResult<PostCreateDraftResponse> {
        let (action, parent_id) =
            reply_or_forward_params.map_or((None, None), |v| (Some(v.action), Some(v.parent_id)));
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
        message_id: MessageId,
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
        message_id: MessageId,
        packages: Vec<Package>,
        auto_save_contacts: Option<bool>,
        delay: Option<Duration>,
        delivery_time: Option<u64>,
        expiration_time: Option<u64>,
    ) -> ApiServiceResult<PostSendMessageResponse> {
        let send_request = PostSendRequest {
            expiration_time,
            expires_in: None,
            auto_save_contacts,
            delay_seconds: delay.map(|v| v.as_secs()),
            delivery_time,
            packages,
        };

        Ok(POST!("{MAIL_V4}/messages/{message_id}")
            .body_json(send_request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn send_direct(
        &self,
        message: DirectParams,
        parent: Option<(MessageId, DraftAction)>,
        packages: Vec<Package>,
        auto_save_contacts: bool,
    ) -> ApiServiceResult<PostSendDirectMessageResponse> {
        let (parent_id, action) =
            parent.map_or_else(|| (None, None), |(id, action)| (Some(id), Some(action)));

        let send_request = PostSendDirectRequest {
            message,
            parent_id,
            action,
            packages,
            auto_save_contacts,
        };

        Ok(POST!("{MAIL_V4}/messages/send/direct")
            .body_json(send_request)?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn cancel_send(&self, message_id: MessageId) -> ApiServiceResult<PostCancelSendResponse> {
        Ok(POST!("{MAIL_V4}/messages/{message_id}/cancel_send")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    /// Reports a message as phishing.
    /// It requires the decrypted message body.
    async fn report_phishing(
        &self,
        message_id: MessageId,
        mime_type: MimeType,
        body: &str,
    ) -> ApiServiceResult<()> {
        let query = json!({
            "MessageID": message_id,
            "MIMEType": mime_type,
            "Body": body
        });

        POST!("{CORE_V4}/reports/phishing")
            .body_json(query)?
            .send_with(self)
            .await?
            .ok()?;
        Ok(())
    }
    async fn delete_all_messages_in_label(&self, label_id: LabelId) -> ApiServiceResult<()> {
        let query = json! ({
            "LabelID": label_id,
        });

        DELETE!("{MAIL_V4}/messages/empty")
            .query(serde_to_query(query)?)
            .send_with(self)
            .await?
            .ok()?;
        Ok(())
    }

    async fn get_unleash_feature_flags(&self) -> ApiServiceResult<GetUnleashFeaturesResponse> {
        Ok(GET!("{UNLEASH_V2}/frontend")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn mark_unsubscribed(&self, ids: Vec<MessageId>) -> ApiServiceResult<()> {
        let body = json!({
            "IDs": ids
        });

        PUT!("{MAIL_V4}/messages/mark/unsubscribed")
            .body_json(body)?
            .send_with(self)
            .await?
            .ok()?;
        Ok(())
    }
}
