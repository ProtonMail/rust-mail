use crate::Result;
use crate::app::{NewChallengeEvent, UserEvent};
use async_trait::async_trait;
use futures::TryFutureExt;
use proton_core_api::verification::{ChallengeNotifier, ChallengePayload, ChallengeResponse};
use serde::Deserialize;
use std::sync::mpsc::channel;
use tao::event_loop::EventLoopProxy;

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

    HumanVerificationSuccess {
        #[serde(rename = "type")]
        ttype: String,
        token: String,
    },

    Error {
        code: String,
        message: String,
    },
}

pub struct HvNotifier {
    tx: EventLoopProxy<UserEvent>,
}

impl HvNotifier {
    pub fn new(tx: EventLoopProxy<UserEvent>) -> Self {
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
impl ChallengeNotifier for HvNotifier {
    async fn on_challenge(&self, payload: ChallengePayload) -> ChallengeResponse {
        self.handle_challenge(payload)
            .inspect_err(|e| error!("failed to handle challenge: {e:?}"))
            .unwrap_or_else(|_| ChallengeResponse::Failure)
            .await
    }
}
