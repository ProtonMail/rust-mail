use crate::actions::MailActionError;
use crate::datatypes::{LocalMessageId, ParsedHeaders};
use anyhow::{Context, anyhow};
use proton_action_queue::action::{Action, DefaultVersionConverter, Type, WriterGuard};
use proton_action_queue::action::{ActionId, Handler};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use stash::params;
use stash::stash::Bond;
use tracing::warn;
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

    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = UnsubscribeNewsletterHandler;
    type RemoteOutput = ();
    type LocalOutput = ();
    type Error = MailActionError;
}

pub struct UnsubscribeNewsletterHandler;

impl Handler for UnsubscribeNewsletterHandler {
    type Action = UnsubscribeNewsletter;

    async fn apply_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        tx.execute(
            "INSERT OR IGNORE INTO unsubscribe (local_message_id) VALUES (?)",
            params![action.id],
        )
        .await?;
        Ok(())
    }

    async fn revert_local(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        tx: &Bond<'_>,
    ) -> Result<(), <Self::Action as Action>::Error> {
        tx.execute(
            "DELETE FROM unsubscribe WHERE local_message_id = ?",
            params![action.id],
        )
        .await?;
        Ok(())
    }

    async fn apply_remote(
        &self,
        _: ActionId,
        action: &mut Self::Action,
        _: WriterGuard<'_>,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        if let Some(url) = &action.request {
            // A GET request to the url should be enough
            _ = reqwest::Client::new()
                .request(Method::GET, url.as_str())
                .send()
                .await
                .context("error sending unsubscribe http request")?
                .error_for_status()
                .context("Server returned error when unsubscribing")?;

            return Ok(());
        }

        Err(anyhow!("Unsubscribe newsletter via email is not yet implemented.").into())
    }
}
