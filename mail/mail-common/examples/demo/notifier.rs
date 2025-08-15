use crate::app::events::{NewChallengeEvent, Proxy, UserEvent};
use anyhow::Result;
use async_trait::async_trait;
use futures::TryFutureExt;
use proton_core_api::verification::*;
use serde::Deserialize;
use std::sync::mpsc::channel;

#[allow(unused)]
#[derive(Debug, Deserialize)]
#[serde(tag = "type", content = "payload")]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum HvMessage {
    Close,

    Loaded,

    Resize {
        height: u32,
    },

    Notification {
        #[serde(rename = "type")]
        ttype: String,
        text: String,
    },

    #[serde(rename = "HUMAN_VERIFICATION_SUCCESS")]
    Success {
        #[serde(rename = "type")]
        ttype: String,
        token: String,
    },

    Error {
        code: String,
        message: String,
    },
}

pub struct HvNotifier<P> {
    tx: P,
}

impl<P: Proxy> HvNotifier<P> {
    pub fn new(tx: P) -> Self {
        Self { tx }
    }

    async fn handle_challenge(&self, payload: ChallengePayload) -> Result<ChallengeResponse> {
        let (tx, rx) = channel::<ChallengeResponse>();

        let event = UserEvent::NewChallenge(NewChallengeEvent { payload, tx });

        match self.tx.send_event(event) {
            Ok(()) => Ok(rx.recv()?),
            Err(_) => Ok(ChallengeResponse::Cancelled),
        }
    }
}

#[async_trait]
impl<P: Proxy> ChallengeNotifier for HvNotifier<P> {
    async fn on_challenge(
        &self,
        _: ChallengeServer,
        payload: ChallengePayload,
    ) -> ChallengeResponse {
        self.handle_challenge(payload)
            .inspect_err(|e| error!("failed to handle challenge: {e:?}"))
            .unwrap_or_else(|_| ChallengeResponse::Failure)
            .await
    }
}
