use crate::MailContextError;
use crate::MailUserContext;
use crate::datatypes::{MimeType, attachment};
use crate::draft::compose::{REPLY_PREFIX, apply_prefix_to_subject, resolve_sender_alias};
use crate::draft::send::MailType;
use crate::draft::{SendError, send};
use crate::models::AttachmentType;
use crate::models::Message;
use crate::models::MessageBodyMetadata;
use crate::models::{Attachment, MailSettings};
use anyhow::Context;
use proton_calendar_common as cal;
use proton_core_api::services::proton::{PrivateEmailRef, PrivateString};
use proton_crypto_account::keys::EmailMimeType;
use proton_crypto_account::keys::PrimaryUnlockedAddressKey;
use proton_crypto_account::keys::UnlockedAddressKeys;
use proton_crypto_inbox::attachment::{EncryptableAttachment, EncryptedAttachment};
use proton_crypto_inbox::keys::ComposerPreference;
use proton_crypto_inbox::message::EncryptableDraft;
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::prelude::{
    DirectAttachment, DirectParams, DraftAction, DraftRecipient, DraftSender, Package,
};
use stash::orm::Model as _;
use stash::stash::Tether;
use std::slice;
use tracing::debug;
use tracing::error;
use tracing::warn;

pub(crate) struct RsvpMail<'a, P>
where
    P: PGPProviderSync,
{
    pub ctx: &'a MailUserContext,
    pub pgp: &'a P,
    pub tether: &'a mut Tether,
    pub msg_id: &'a MessageId,
    pub msg_meta: &'a MessageBodyMetadata,
    pub msg_subject: &'a str,
    pub addr_keys: &'a UnlockedAddressKeys<P>,
    pub addr_email: &'a str,
    pub addr_display_name: &'a str,
}

impl<P> cal::RsvpMail for RsvpMail<'_, P>
where
    P: PGPProviderSync,
{
    type Error = MailContextError;

    async fn send(mut self, to: &str, body: &str, ics: &str) -> Result<(), Self::Error> {
        let key = {
            debug!("Getting mail key");

            self.addr_keys
                .primary_for_mail()
                .context("Couldn't get primary key")
                .map_err(MailContextError::Other)?
        };

        let ics = {
            debug!("Encrypting attachment");

            RsvpAttachment(ics).attachment_encrypt_and_sign(self.pgp, &key)?
        };

        let message = self.build_message(to.into(), body, &key, &ics)?;
        let parent = Some((self.msg_id.clone(), DraftAction::Reply));
        let (packages, mut ics) = self.build_packages(to.into(), body, ics).await?;
        let auto_save_contacts = false;

        let resp = {
            debug!("Sending mail");

            self.ctx
                .session()
                .send_direct(message, parent, packages, auto_save_contacts)
                .await?
        };

        let result = self
            .tether
            .tx::<_, _, anyhow::Error>(async move |tx| {
                debug!("Saving message into the database");

                if let Some(remote_att) = resp.sent.body.attachments.first() {
                    ics.attachment_type = AttachmentType::Remote(Some(remote_att.id.clone()));
                } else {
                    warn!("Suspicious: API response contains no attachments");
                }

                let (mut msg, mut body, _) = Message::from_api_data(resp.sent, tx)
                    .await
                    .context("Couldn't create message from API response")?;

                msg.save(tx).await.context("Couldn't save message")?;
                body.save(tx).await.context("Couldn't save message body")?;

                Ok(())
            })
            .await;

        if let Err(err) = result {
            // Since the reply was already sent to the organizer, we can't bail
            // out now - the organizer's calendar got updated (by the virtue of
            // getting the mail sent), so our calendar must be updated as well.
            //
            // (i.e. if we bailed out here, RSVP logic would assume that the
            // mail did not, in fact, go through, and would abort the flow.)
            //
            // It's a pity, sure, but let's hope that event loop catches this
            // and pulls the message anyway.

            warn!(
                "Message to the organizer got sent correctly, but we couldn't \
                 save it into the database: {err:?}",
            );
        }

        Ok(())
    }
}

impl<P> RsvpMail<'_, P>
where
    P: PGPProviderSync,
{
    #[allow(clippy::result_large_err)]
    fn build_message(
        &self,
        to: PrivateEmailRef,
        body: &str,
        key: &PrimaryUnlockedAddressKey<P::PrivateKey, P::PublicKey>,
        ics: &EncryptedAttachment,
    ) -> Result<DirectParams, MailContextError> {
        debug!("Building message");

        let subject = apply_prefix_to_subject(REPLY_PREFIX, self.msg_subject);
        let from = resolve_sender_alias(self.addr_email, self.msg_meta);

        let sender = DraftSender {
            address: from.into(),
            name: self.addr_display_name.into(),
        };

        let body = RsvpBody(body)
            .encrypt_draft_body(self.pgp, key)
            .map_err(|err| {
                error!("Failed to encrypt response: {err:?}");
                MailContextError::Crypto
            })?;

        let to = DraftRecipient {
            address: to.to_owned(),
            name: PrivateString::default(),
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
        to: PrivateEmailRef<'_>,
        body: &str,
        ics: EncryptedAttachment,
    ) -> Result<(Vec<Package>, Attachment), MailContextError> {
        debug!("Building packages");

        let to = to.to_owned();

        let ics = self
            .tether
            .tx(async |bond| {
                Attachment::create(
                    self.ctx,
                    bond,
                    ics,
                    DirectAttachment::INVITE_ICS,
                    attachment::MimeType::text_plain(),
                )
                .await
            })
            .await?;

        let crypto = MailSettings::get(self.tether)
            .await?
            .unwrap_or_default()
            .crypto_mail_settings();

        let prefs = send::load_prefs(
            self.ctx,
            self.pgp,
            self.tether,
            slice::from_ref(&to),
            crypto,
            ComposerPreference::new(EmailMimeType::Text),
        )
        .await?;

        let packages = send::build_packages(
            self.ctx,
            MailType::Direct,
            self.pgp,
            self.addr_keys,
            prefs,
            MimeType::TextPlain,
            body,
            slice::from_ref(&ics),
            None,
            self.tether,
        )
        .await
        .map_err(SendError::SendMessage)?;

        Ok((packages, ics))
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
