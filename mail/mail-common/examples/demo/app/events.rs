use anyhow::Result;
use proton_core_api::verification::{ChallengePayload, ChallengeResponse};
use std::sync::mpsc::Sender;

#[derive(Debug)]
#[allow(unused)]
pub enum UserEvent {
    Exit,
    NewChallenge(NewChallengeEvent),
    EndChallenge(EndChallengeEvent),
}

#[derive(Debug)]
#[allow(unused)]
pub struct NewChallengeEvent {
    pub payload: ChallengePayload,
    pub tx: Sender<ChallengeResponse>,
}

#[derive(Debug)]
#[allow(unused)]
pub struct EndChallengeEvent {
    pub response: ChallengeResponse,
    pub tx: Sender<ChallengeResponse>,
}

pub trait Proxy: Clone + Send + Sync {
    fn send_event(&self, _: UserEvent) -> Result<()> {
        Ok(())
    }
}
