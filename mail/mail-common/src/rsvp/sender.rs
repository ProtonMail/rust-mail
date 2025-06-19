use crate::MailContextError;
use crate::MailUserContext;
use crate::datatypes::{MessageRecipient, MimeType, attachment};
use crate::draft::compose::REPLY_PREFIX;
use crate::draft::{self, send};
use crate::models::{Attachment, MailSettings};
use anyhow::Context;
use proton_calendar_common::{self as cal};
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::AddressId;
use proton_crypto_account::keys::{PrimaryUnlockedAddressKey, UnlockedAddressKeys};
use proton_crypto_inbox::attachment::{EncryptableAttachment, EncryptedAttachment};
use proton_crypto_inbox::message::EncryptableDraft;
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::prelude::{
    DirectAttachment, DirectParams, DraftAction, DraftRecipient, DraftSender, Package,
};
use stash::stash::Tether;
use std::slice;
use thiserror::Error;

pub(crate) struct RsvpMailSender<'a, P>
where
    P: PGPProviderSync,
{
    pub ctx: &'a MailUserContext,
    pub pgp: &'a P,
    pub keys: &'a UnlockedAddressKeys<P>,
    pub tether: &'a mut Tether,
    pub msg_id: &'a MessageId,
    pub msg_subject: &'a str,
    pub msg_recipient: &'a MessageRecipient,
    pub msg_address_id: &'a AddressId,
}

impl<P> cal::RsvpMailSender for RsvpMailSender<'_, P>
where
    P: PGPProviderSync,
{
    type Error = RsvpMailError;

    async fn send(mut self, to: &str, body: &str, ics: &str) -> Result<(), RsvpMailError> {
        let key = self.keys.primary_for_mail().with_context(|| {
            format!(
                "Couldn't get primary key for address {}",
                self.msg_address_id
            )
        })?;

        let ics = RsvpAttachment(ics)
            .attachment_encrypt_and_sign(self.pgp, &key)
            .context("Couldn't encrypt attachment")?;

        let message = self.build_message(to, body, &key, &ics)?;
        let parent = Some((self.msg_id.clone(), DraftAction::Reply));
        let packages = self.build_packages(to, body, &ics).await?;
        let auto_save_contacts = false;

        self.ctx
            .api()
            .send_direct_mail(message, parent, packages, auto_save_contacts)
            .await?;

        Ok(())
    }
}

impl<P> RsvpMailSender<'_, P>
where
    P: PGPProviderSync,
{
    fn build_message(
        &self,
        to: &str,
        body: &str,
        key: &PrimaryUnlockedAddressKey<P::PrivateKey, P::PublicKey>,
        ics: &EncryptedAttachment,
    ) -> Result<DirectParams, RsvpMailError> {
        let subject = draft::compose::apply_prefix_to_subject(REPLY_PREFIX, self.msg_subject);

        let sender = DraftSender {
            address: self.msg_recipient.address.clone(),
            name: self.msg_recipient.name.clone(),
        };

        let body = RsvpBody(body)
            .encrypt_draft_body(self.pgp, key)
            .context("Couldn't encrypt body")?;

        let to = DraftRecipient {
            address: to.to_string(),
            name: to.to_string(),
            group: None,
        };

        let attachment = DirectAttachment::invite_reply(ics);

        Ok(DirectParams {
            subject,
            sender,
            to_list: vec![to],
            body,
            attachments: vec![attachment],
        })
    }

    async fn build_packages(
        &mut self,
        to: &str,
        body: &str,
        ics: &EncryptedAttachment,
    ) -> Result<Vec<Package>, RsvpMailError> {
        let to = to.to_string();
        let ics = Attachment::direct(ics, attachment::MimeType::text_plain());

        let crypto = MailSettings::get(self.tether)
            .await
            .context("Couldn't get mail settings")?
            .unwrap_or_default()
            .crypto_mail_settings();

        let prefs = send::load_send_preferences_for_recipients(
            self.ctx,
            self.pgp,
            self.tether,
            slice::from_ref(&to),
            crypto,
        )
        .await
        .context("Couldn't load preferences")?;

        let packages = send::build_packages(
            self.ctx,
            self.pgp,
            self.keys,
            prefs,
            MimeType::TextPlain,
            body,
            slice::from_ref(&ics),
            self.tether,
        )
        .await
        .context("Couldn't build packages")?;

        Ok(packages)
    }
}

#[derive(Clone, Debug)]
struct RsvpAttachment<'a>(&'a str);

impl EncryptableAttachment for RsvpAttachment<'_> {
    fn attachment_data(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

#[derive(Clone, Debug)]
struct RsvpBody<'a>(&'a str);

impl EncryptableDraft for RsvpBody<'_> {
    fn plaintext_message_body(&self) -> &[u8] {
        self.0.as_bytes()
    }
}

#[derive(Debug, Error)]
pub enum RsvpMailError {
    #[error(transparent)]
    Api(#[from] ApiServiceError),

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl From<RsvpMailError> for MailContextError {
    fn from(err: RsvpMailError) -> Self {
        match err {
            RsvpMailError::Api(err) => MailContextError::Api(err),
            RsvpMailError::Other(err) => MailContextError::Other(err),
        }
    }
}
