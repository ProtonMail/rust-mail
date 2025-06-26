use crate::MailContextError;
use crate::MailUserContext;
use crate::datatypes::{MessageRecipient, MimeType, attachment};
use crate::draft::SendError;
use crate::draft::compose::REPLY_PREFIX;
use crate::draft::send::MailType;
use crate::draft::{self, send};
use crate::models::AttachmentType;
use crate::models::Message;
use crate::models::{Attachment, MailSettings};
use anyhow::Context;
use proton_calendar_common::{self as cal};
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
use tracing::debug;
use tracing::error;
use tracing::warn;

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
    type Error = MailContextError;

    async fn send(mut self, to: &str, body: &str, ics: &str) -> Result<(), Self::Error> {
        let key = {
            debug!("Getting mail key");

            self.keys
                .primary_for_mail()
                .with_context(|| {
                    format!(
                        "Couldn't get primary key for address {}",
                        self.msg_address_id
                    )
                })
                .map_err(MailContextError::Other)?
        };

        let ics = {
            debug!("Encrypting attachment");

            RsvpAttachment(ics).attachment_encrypt_and_sign(self.pgp, &key)?
        };

        let message = {
            debug!("Building message");

            self.build_message(to, body, &key, &ics)?
        };

        let parent = Some((self.msg_id.clone(), DraftAction::Reply));

        let (packages, mut ics) = {
            debug!("Building packages");

            self.build_packages(to, body, ics).await?
        };

        let auto_save_contacts = false;

        let resp = {
            debug!("Sending mail");

            self.ctx
                .api()
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
    ) -> Result<DirectParams, MailContextError> {
        let subject = draft::compose::apply_prefix_to_subject(REPLY_PREFIX, self.msg_subject);

        let sender = DraftSender {
            address: self.msg_recipient.address.clone(),
            name: self.msg_recipient.name.clone(),
        };

        let body = RsvpBody(body)
            .encrypt_draft_body(self.pgp, key)
            .map_err(|err| {
                error!("Failed to encrypt response: {err:?}");
                MailContextError::Crypto
            })?;

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
        ics: EncryptedAttachment,
    ) -> Result<(Vec<Package>, Attachment), MailContextError> {
        let to = to.to_string();

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
        )
        .await?;

        let packages = send::build_packages(
            self.ctx,
            MailType::Direct,
            self.pgp,
            self.keys,
            prefs,
            MimeType::TextPlain,
            body,
            slice::from_ref(&ics),
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
