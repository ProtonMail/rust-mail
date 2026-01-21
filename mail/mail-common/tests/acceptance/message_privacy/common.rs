use proton_core_api::services::proton::AddressId;
use proton_crypto_account::keys::{LocalAddressKey, LocalUserKey, UnlockedAddressKeys};
use proton_crypto_inbox::message::EncryptableDraft;
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_inbox::proton_crypto::new_pgp_provider;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::response_data::{
    MailSettings as ApiMailSettings, Message as ApiMessage, MessageBody as ApiMessageBody,
    MessageFlags as ApiMessageFlags, MessageMetadata as ApiMessageMetadata,
    MessageSender as ApiMessageSender, MimeType as ApiMimeType, ViewMode as ApiViewMode,
};
use proton_mail_common::datatypes::{
    LocalMessageId, PrivacyInfoStatus, StrippedUTMInfo, TrackerInfo,
};
use proton_mail_common::test_utils::init::Params;
use proton_mail_common::test_utils::message_body::{
    TEST_USER_ADDRESS_ID, message_body_test_addresses, message_body_test_user_info,
    message_body_test_user_secret,
};
use proton_mail_common::{PrivacyWatchData, TrackerService};
use std::collections::HashMap;
use std::time::Duration;
use tokio::time::sleep;

pub async fn get_or_wait_for_only_utm_links(
    id: LocalMessageId,
    service: &TrackerService,
    PrivacyWatchData { initial, handle }: PrivacyWatchData,
) -> anyhow::Result<StrippedUTMInfo> {
    let mut data = initial;
    let timeout = sleep(Duration::from_secs(5));
    tokio::pin!(timeout);
    loop {
        if let Some(utm_links) = data.utm_links {
            return Ok(utm_links);
        }

        tokio::select! {
            _ = &mut timeout => anyhow::bail!("Timeout waiting for table changes"),
            res = handle.receiver.recv_async() => match res {
                Err(_) => anyhow::bail!("Channel closed"),
                Ok(()) => {
                    let new_data = service.get_info(id).await?;
                    data = new_data;
                }
            },
        }
    }
}
pub async fn get_or_wait_for_privacy_data(
    id: LocalMessageId,
    service: &TrackerService,
    PrivacyWatchData { initial, handle }: PrivacyWatchData,
) -> anyhow::Result<(TrackerInfo, StrippedUTMInfo)> {
    let mut data = initial;
    let timeout = sleep(Duration::from_secs(5));
    tokio::pin!(timeout);
    loop {
        if let PrivacyInfoStatus::Detected(trackers) = data.trackers
            && let Some(utm_links) = data.utm_links
        {
            return Ok((trackers, utm_links));
        }

        tokio::select! {
            _ = &mut timeout => anyhow::bail!("Timeout waiting for table changes"),
            res = handle.receiver.recv_async() => match res {
                Err(_) => anyhow::bail!("Channel closed"),
                Ok(()) => {
                    let new_data = service.get_info(id).await?;
                    data = new_data;
                }
            },
        }
    }
}

pub fn test_params() -> Params {
    use proton_core_common::datatypes::ImageProxy;
    Params {
        user_info: Some(message_body_test_user_info()),
        addresses: message_body_test_addresses(),
        mail_settings: Some(ApiMailSettings {
            view_mode: ApiViewMode::Messages,
            image_proxy: ImageProxy::ENABLED.bits(),
            ..Default::default()
        }),
        ..Default::default()
    }
}

pub fn test_params_proxy_disabled() -> Params {
    Params {
        user_info: Some(message_body_test_user_info()),
        addresses: message_body_test_addresses(),
        mail_settings: Some(ApiMailSettings {
            view_mode: ApiViewMode::Messages,
            image_proxy: 0,
            ..Default::default()
        }),
        ..Default::default()
    }
}

struct HtmlBody {
    body: String,
}

impl EncryptableDraft for HtmlBody {
    fn plaintext_message_body(&self) -> &[u8] {
        self.body.as_bytes()
    }
}

fn create_message_with_html_body_impl<P: PGPProviderSync>(
    provider: &P,
    message_id: &str,
    html: &str,
) -> ApiMessage {
    let user_secret = message_body_test_user_secret();
    let user_info = message_body_test_user_info();
    let addresses = message_body_test_addresses();

    let user_key_locked = &user_info.keys.0[0];
    let local_user_key = LocalUserKey {
        private_key: user_key_locked.private_key.clone(),
    };
    let unlocked_user_key = local_user_key
        .unlock_and_assign_key_id(provider, user_key_locked.id.clone(), &user_secret.0)
        .expect("Failed to unlock user key");

    let address_key_locked = &addresses[0].keys.0[0];
    let local_address_key = LocalAddressKey {
        private_key: address_key_locked.private_key.clone(),
        token: address_key_locked.token.clone(),
        signature: address_key_locked.signature.clone(),
        flags: address_key_locked.flags.unwrap_or_default(),
        primary: address_key_locked.primary,
    };

    let unlocked_address_key = local_address_key
        .unlock_and_assign_key_id(provider, address_key_locked.id.clone(), &unlocked_user_key)
        .expect("Failed to unlock address key");

    let unlocked_address_keys: UnlockedAddressKeys<P> =
        UnlockedAddressKeys::from(unlocked_address_key);
    let primary_key = unlocked_address_keys
        .primary_for_mail()
        .expect("Failed to get primary key");

    let html_body = HtmlBody {
        body: html.to_string(),
    };

    let encrypted = html_body
        .encrypt_draft_body(provider, &primary_key)
        .expect("Failed to encrypt");

    ApiMessage {
        metadata: ApiMessageMetadata {
            id: MessageId::from(message_id),
            conversation_id: ConversationId::from("test_conversation"),
            order: 0,
            address_id: AddressId::from(TEST_USER_ADDRESS_ID),
            label_ids: vec![],
            external_id: None,
            subject: "Test Message".to_owned(),
            sender: ApiMessageSender::default(),
            to_list: vec![],
            cc_list: vec![],
            bcc_list: vec![],
            flags: ApiMessageFlags::empty(),
            time: 1715863508,
            size: 500,
            unread: false,
            is_replied: false,
            is_replied_all: false,
            is_forwarded: false,
            expiration_time: 0,
            snooze_time: 0,
            num_attachments: 0,
            attachments_metadata: vec![],
        },
        body: ApiMessageBody {
            header: String::new(),
            parsed_headers: HashMap::default(),
            body: encrypted.0,
            mime_type: ApiMimeType::TextHtml,
            attachments: vec![],
            reply_to: Default::default(),
            reply_tos: vec![],
        },
    }
}

pub fn create_message_with_html_body(message_id: &str, html: &str) -> ApiMessage {
    let provider = new_pgp_provider();
    create_message_with_html_body_impl(&provider, message_id, html)
}
