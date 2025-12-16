//! Utilities to listen to the proton event loop. This crate provides an event polling system
//! that can handle multiple event types through the [`EventManager`](`crate::v6::EventManager`)
//! which is the main entry point to this crate.
//!
//! The system works with [`RawEvent`]s that are then converted to specific event types by registered
//! subscribers.
//!
//!
//! The event polling system uses a two-tier approach:
//! - **Raw Events**: Events are initially fetched as [`RawEvent`]s from the API
//! - **Typed Events**: Each [`v6::EventSource`] deserializes the [`RawEvent`] into a typed version that
//!   is then consumed by the registered [`Subscriber`]s
//!
//!
//! For more details see the [`v6 module`](`crate::v6`) documentation.
//!
pub mod provider;
pub mod store;
pub mod v6;

use std::fmt;
// Re-export main types
pub use provider::{EventProvider, EventProviderError, EventProviderResult};

use anyhow::Error as AnyhowError;
use serde::{Deserialize, Serialize};
use std::fmt::{Debug, Formatter};
use thiserror::Error;
pub use v6::EventSubscriberError;
pub use v6::EventSubscriberResult;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct EventId(String);

impl EventId {
    #[must_use]
    pub fn into_inner(self) -> String {
        self.0
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<T: Into<String>> From<T> for EventId {
    fn from(value: T) -> Self {
        Self(value.into())
    }
}

impl AsRef<str> for EventId {
    fn as_ref(&self) -> &str {
        self.0.as_ref()
    }
}

impl fmt::Display for EventId {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

#[derive(Debug, Error)]
pub enum EventLoopError {
    #[error("Subscriber ({0}) failed to apply refresh event: {1}")]
    Refresh(String, Box<dyn EventSubscriberError>),
    #[error("Failed to read/write from/to store: {0}")]
    Store(AnyhowError),
    #[error("Failed to retrieve event: {0}")]
    Provider(Box<dyn EventProviderError>),
    #[error("Subscriber ({0}) failed to apply event: {1}")]
    Subscriber(String, Box<dyn EventSubscriberError>),
    #[error("Subscriber with `{0}` name already exists")]
    Register(&'static str),
    #[error("Failed to deserialize event: {0}")]
    Deserialize(AnyhowError),
    #[error("Cyclic dependency detected between event sources")]
    CyclicDependency,
    #[error("Event source {0} already registered")]
    DuplicateEventSource(&'static str),
    #[error("Failed to communicate with actor")]
    Actor,
}

impl From<Box<dyn EventProviderError>> for EventLoopError {
    fn from(err: Box<dyn EventProviderError>) -> Self {
        EventLoopError::Provider(err)
    }
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RawEvent {
    meta: EventMetadata,
    raw: String,
}

impl RawEvent {
    pub fn from_json(raw: String) -> Result<Self, AnyhowError> {
        Ok(Self {
            meta: serde_json::from_str(&raw)?,
            raw,
        })
    }

    pub fn deserialize<'a, T: Deserialize<'a>>(&'a self) -> Result<T, serde_json::Error> {
        serde_json::from_str(&self.raw)
    }
}
impl RawEvent {
    fn event_id(&self) -> EventId {
        self.meta.event_id.clone().into_inner().into()
    }

    fn has_more(&self) -> bool {
        self.meta.has_more.as_bool()
    }

    fn is_refresh(&self) -> bool {
        self.meta.refresh.as_bool()
    }

    fn refresh_flag(&self) -> RefreshFlag {
        self.meta.refresh
    }
}

#[derive(Debug, Clone, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
struct EventMetadata {
    #[serde(rename = "EventID")]
    event_id: EventId,
    #[serde(rename = "More")]
    has_more: RefreshFlag,
    refresh: RefreshFlag,
}

/// Compatability type to handle differences in the refresh value between
/// v5 and v6 loops.
#[derive(Debug, Copy, Clone, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "PascalCase")]
#[serde(untagged)]
pub enum RefreshFlag {
    Bool(bool),
    Integer(u8),
}

impl RefreshFlag {
    #[must_use]
    pub fn as_bool(self) -> bool {
        match self {
            RefreshFlag::Bool(v) => v,
            RefreshFlag::Integer(v) => v != 0,
        }
    }

    #[must_use]
    pub fn as_u8(self) -> u8 {
        match self {
            RefreshFlag::Bool(v) => {
                if v {
                    u8::MAX
                } else {
                    0
                }
            }
            RefreshFlag::Integer(v) => v,
        }
    }
}

impl From<bool> for RefreshFlag {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<u8> for RefreshFlag {
    fn from(value: u8) -> Self {
        Self::Integer(value)
    }
}

pub const MAX_ERROR_RETRIES: usize = 3;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn event_control_data_deserialize() {
        let event_data_v5 = r#"{"EventID":"foo", "More":1, "Refresh":255}"#;
        let event_data_v6 = r#"{"EventID":"foo", "More":true, "Refresh":true}"#;

        let event_data_v5 = RawEvent::from_json(event_data_v5.into()).unwrap();
        let event_data_v6 = RawEvent::from_json(event_data_v6.into()).unwrap();

        assert!(event_data_v5.is_refresh());
        assert!(event_data_v5.has_more());
        assert!(event_data_v6.is_refresh());
        assert!(event_data_v6.has_more());

        let event_data_v5 = r#"{"EventID":"foo", "More":0, "Refresh":0}"#;
        let event_data_v6 = r#"{"EventID":"foo", "More":false, "Refresh":false}"#;

        let event_data_v5 = RawEvent::from_json(event_data_v5.into()).unwrap();
        let event_data_v6 = RawEvent::from_json(event_data_v6.into()).unwrap();

        assert!(!event_data_v5.is_refresh());
        assert!(!event_data_v5.has_more());
        assert!(!event_data_v6.is_refresh());
        assert!(!event_data_v6.has_more());
    }
}
