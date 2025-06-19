use crate::MailContextError;
use crate::datatypes::{MimeType, attachment};
use crate::draft::compose::REPLY_PREFIX;
use crate::draft::{self, send};
use crate::models::{Attachment, MailSettings};
use crate::{MailUserContext, models::Message};
use anyhow::Context;
use proton_calendar_api::{CalendarBootstrap, CalendarId};
use proton_calendar_common as calendar;
use proton_core_api::service::ApiServiceError;
use proton_crypto_account::keys::{PrimaryUnlockedAddressKey, UnlockedAddressKeys};
use proton_crypto_inbox::attachment::{EncryptableAttachment, EncryptedAttachment};
use proton_crypto_inbox::message::EncryptableDraft;
use proton_crypto_inbox::proton_crypto::crypto::PGPProviderSync;
use proton_mail_api::services::proton::ProtonMail;
use proton_mail_api::services::proton::prelude::{
    DirectAttachment, DirectParams, DraftAction, DraftRecipient, DraftSender, Package,
};
use stash::stash::Tether;
use std::collections::HashMap;
use std::slice;
use thiserror::Error;
use tokio::sync::Mutex;

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

impl calendar::RsvpCache for RsvpCache {
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
    pub msg: &'a Message,
    pub pgp: &'a P,
    pub keys: &'a UnlockedAddressKeys<P>,
    pub tether: &'a mut Tether,
}

impl<P> calendar::RsvpMailSender for RsvpMailSender<'_, P>
where
    P: PGPProviderSync,
{
    type Error = RsvpMailError;

    async fn send(mut self, to: &str, body: &str, ics: &str) -> Result<(), RsvpMailError> {
        let remote_id = self
            .msg
            .remote_id
            .clone()
            .context("Message has no remote id")?;

        let key = self.keys.primary_for_mail().with_context(|| {
            format!(
                "Couldn't get primary key for address {}",
                self.msg.remote_address_id
            )
        })?;

        let ics = RsvpAttachment(ics)
            .attachment_encrypt_and_sign(self.pgp, &key)
            .context("Couldn't encrypt attachment")?;

        let message = self.build_message(to, body, &key, &ics)?;
        let parent = Some((remote_id, DraftAction::Reply));
        let packages = self.build_packages(to, body, &ics).await?;
        let attachment_keys = Vec::new();
        let auto_save_contacts = false;

        self.ctx
            .api()
            .send_direct_mail(
                message,
                parent,
                packages,
                attachment_keys,
                auto_save_contacts,
            )
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
        let subject = draft::compose::apply_prefix_to_subject(REPLY_PREFIX, &self.msg.subject);

        let body = RsvpBody(body)
            .encrypt_draft_body(self.pgp, key)
            .context("Couldn't encrypt body")?;

        let sender = {
            let sender = self
                .msg
                .to_list
                .value
                .first()
                .context("Message has no sender")?;

            DraftSender {
                address: sender.address.clone(),
                name: sender.name.clone(),
            }
        };

        let to = DraftRecipient {
            address: to.to_string(),
            name: to.to_string(),
            group: None,
        };

        let ics = DirectAttachment::invite_reply(ics);

        Ok(DirectParams {
            subject,
            sender,
            to_list: vec![to],
            body,
            attachments: vec![ics],
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
