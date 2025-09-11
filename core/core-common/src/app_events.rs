//! These events are not related to the events that arrive as updates to proton state. They
//! are triggered during the execution of an application.

use proton_core_api::services::proton::{SessionId, UserId};

#[derive(Debug, Clone)]
pub struct UserSessionDeletedEvent {
    pub session_id: SessionId,
    pub user_id: UserId,
}

#[derive(Debug, Clone)]
pub struct UserSessionCreatedEvent {
    pub session_id: SessionId,
    pub user_id: UserId,
}

#[derive(Debug, Copy, Clone)]
pub struct OnEnterForegroundEvent;

#[derive(Debug, Copy, Clone)]
pub struct OnExitForegroundEvent;
