use crate::AppError;
use crate::actions::MailActionError;
use crate::datatypes::{LocalMessageId, MessageFlags, ParsedHeaders};
use crate::models::Message;
use anyhow::anyhow;
use proton_action_queue::action::{Action, DefaultVersionConverter, Type, WriterGuard};
use proton_action_queue::action::{ActionId, Handler};
use proton_core_api::service::ApiServiceError;
use proton_core_api::session::Session;
use proton_core_common::models::ModelIdExtension;
use proton_mail_api::services::proton::ProtonMail;
use reqwest::Method;
use serde::{Deserialize, Serialize};
use stash::stash::Bond;
use tracing::{debug, warn};
use url::Url;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct UnsubscribeNewsletter {
    pub mail: Option<Url>,
    pub request: Option<Url>,
    id: LocalMessageId,
}

impl UnsubscribeNewsletter {
    pub fn new(headers: &ParsedHeaders, id: LocalMessageId) -> Option<Self> {
        let Some(list) = headers.headers.get("List-Unsubscribe") else {
            tracing::trace!("message has no List-Unsubscribe in headers");
            return None;
        };

        let Some(list) = list.as_str() else {
            tracing::warn!("List-Unsubscribe is not a string.");
            return None;
        };

        let mut res = Self {
            id,
            mail: None,
            request: None,
        };

        for mut value in list.split(',') {
            value = value.trim();
            if value.starts_with('<') && value.ends_with('>') {
                value = &value[1..value.len() - 1];
            }

            let url = match Url::try_from(value) {
                Ok(url) => url,
                Err(e) => {
                    warn!("Could not parse url {value}: {e:?}");
                    continue;
                }
            };

            if url.scheme() == "mailto" {
                res.mail = Some(url);
            } else {
                res.request = Some(url);
            }
        }
        if res.request.is_none()
        // TODO: implment mail unsubscribe
        /* && res.mail.is_none() */
        {
            if res.mail.is_none() {
                tracing::warn!("Unsubscribe via mail is not implemented");
            } else {
                tracing::warn!("Badly formed List-Unsubscribe: {list}");
            }
            return None;
        }

        Some(res)
    }
}

impl Action for UnsubscribeNewsletter {
    const TYPE: Type = Type("unsubscribe_to_newsletter");
    const VERSION: u32 = 1;
    const MAX_RETRIES: Option<u32> = Some(3);

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = UnsubscribeNewsletterHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct UnsubscribeNewsletterHandler {
    pub http_client: reqwest::Client,
    pub api: Session,
}

impl Handler for UnsubscribeNewsletterHandler {
    type Action = UnsubscribeNewsletter;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::set_flags(action.id, MessageFlags::UNSUBSCRIBED, tx).await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Message::unset_flags(action.id, MessageFlags::UNSUBSCRIBED, tx).await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        g: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        if let Some(url) = &action.request {
            debug!("sending unsubscribe request to {url}");
            // A GET request to the url should be enough
            _ = self
                .http_client
                .request(Method::GET, url.as_str())
                .send()
                .await
                .map_err(|e| ApiServiceError::ConnectionError(e.to_string()))?
                .error_for_status()
                .map_err(|e| {
                    ApiServiceError::UnknownError(format!(
                        "Server returned error when unsubscribing: {e:?}"
                    ))
                })?;

            let remote_msg_id = Message::local_id_counterpart(action.id, g.tether())
                .await?
                .ok_or(AppError::MessageHasNoRemoteId(action.id))?;
            self.api.mark_unsubscribed(vec![remote_msg_id]).await?;

            return Ok(());
        }

        Err(anyhow!("Unsubscribe newsletter via email is not yet implemented.").into())
    }

    async fn rebase_local(
        &self,
        _: ActionId,
        _: &mut Self::Action,
        _: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        Ok(())
    }
}
