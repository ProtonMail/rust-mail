use crate::datatypes::{MessageRecipient, MimeType, attachment};
use crate::draft::compose::REPLY_PREFIX;
use crate::draft::{self, send};
use crate::models::{Attachment, MailSettings};
use crate::{MailContextError, MailContextResult};
use crate::{MailUserContext, models::Message};
use anyhow::Context;
use proton_calendar_api::{CalendarBootstrap, CalendarId};
use proton_calendar_common::{self as cal, RsvpAnswer, RsvpAnswerError, RsvpAnswerStatus};
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::AddressId;
use proton_crypto_account::keys::{PrimaryUnlockedAddressKey, UnlockedAddressKeys};
use proton_crypto_inbox::attachment::{EncryptableAttachment, EncryptedAttachment};
use proton_crypto_inbox::message::EncryptableDraft;
use proton_crypto_inbox::proton_crypto;
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::common::MessageId;
use proton_mail_api::services::proton::prelude::{
    DirectAttachment, DirectParams, DraftAction, DraftRecipient, DraftSender, Package,
};
use stash::stash::Tether;
use std::collections::HashMap;
use std::{ops, slice};
use thiserror::Error;
use tokio::sync::Mutex;
use tracing::{info, warn};

#[derive(Clone, Debug)]
pub struct RsvpEvent {
    event: cal::RsvpEvent,
    msg_id: Option<MessageId>,
    msg_subject: String,
    msg_recipient: Option<MessageRecipient>,
    msg_address_id: AddressId,
}

impl RsvpEvent {
    pub(crate) fn new(event: cal::RsvpEvent, msg: &Message) -> Self {
        Self {
            event,
            msg_id: msg.remote_id.clone(),
            msg_subject: msg.subject.clone(),
            msg_recipient: msg.to_list.value.first().cloned(),
            msg_address_id: msg.remote_address_id.clone(),
        }
    }

    /// TODO (NGC-57) implement support for offline-mode
    #[tracing::instrument(
        skip(self, ctx, tether),
        fields(id = self.event.raw.id.as_str()),
    )]
    pub async fn answer(
        &mut self,
        ctx: &MailUserContext,
        tether: &mut Tether,
        status: RsvpAnswerStatus,
    ) -> MailContextResult<()> {
        info!("Answering RSVP");

        let pgp = proton_crypto::new_pgp_provider();

        let keys = ctx
            .unlocked_address_keys(&pgp, tether, &self.msg_address_id)
            .await
            .map_err(|err| {
                warn!(?err, "Couldn't unlock address keys");
                err
            })?;

        let sender = {
            let msg_id = self
                .msg_id
                .as_ref()
                .context("Invite message has no remote id")
                .map_err(MailContextError::Other)?;

            let msg_recipient = self
                .msg_recipient
                .as_ref()
                .context("Invite message has no recipient")
                .map_err(MailContextError::Other)?;

            RsvpMailSender {
                ctx,
                pgp: &pgp,
                keys: &keys,
                tether,
                msg_id,
                msg_subject: &self.msg_subject,
                msg_recipient,
                msg_address_id: &self.msg_address_id,
            }
        };

        let answer = RsvpAnswer {
            now: ctx.mail_context().core_context().clock().now(),
            email: &sender.msg_recipient.address,
            status,
        };

        self.event
            .answer(ctx.api(), &pgp, &keys, ctx.rsvp_cache(), sender, answer)
            .await
            .map_err(|err| match err {
                RsvpAnswerError::Rsvp(err) => err.into(),
                RsvpAnswerError::Mail(err) => err.into(),
            })
    }
}

impl ops::Deref for RsvpEvent {
    type Target = cal::RsvpEvent;

    fn deref(&self) -> &Self::Target {
        &self.event
    }
}

/// Write-once cache for calendar bootstrap data.
///
/// Decryping events requires access to the calendar key which is provided by
/// the calendar bootstrap data - fetching this bootstrap data takes a moment,
/// hence we have this struct which is responsible for caching them bootstraps.
///
/// Note that this is a rudimentary implementation - in particular, if calendar
/// key gets rotated, we will continue to serve the old one until user restarts
/// the application, since we don't listen to server events in here.
///
/// Fortunately, calendar keys are almost never rotated (:fingers-crossed:) and
/// even if they are, restarting the application will reset this cache, causing
/// it to download the current key, no harm done.
///
/// This will be implemented properly over NGC-57, where we'll store the keys
/// into the local database and listen on the event loop - at the moment it's
/// more of a "good enough for the time being" kind of code.
///
/// TODO (NGC-57) implement support for offline-mode
#[derive(Debug, Default)]
pub(crate) struct RsvpCache {
    calendars: Mutex<HashMap<CalendarId, CalendarBootstrap>>,
}

impl cal::RsvpCache for RsvpCache {
    async fn get_calendar_bootstrap<E, Fn, Fut>(
        &self,
        id: &CalendarId,
        fetch: Fn,
    ) -> Result<CalendarBootstrap, E>
    where
        Fn: FnOnce() -> Fut + Send,
        Fut: Future<Output = Result<CalendarBootstrap, E>> + Send,
    {
        let mut calendars = self.calendars.lock().await;

        if let Some(calendar) = calendars.get(id) {
            Ok(calendar.clone())
        } else {
            let calendar = fetch().await?;

            calendars.insert(id.clone(), calendar.clone());

            Ok(calendar)
        }
    }
}

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
