//! Event data types for the Proton Core common library.
//!
//! This module contains various data types used by the Proton Core common
//! library in association to events. Notably, they are NOT used by the models
//! in the [`models`](crate::models) module, i.e. they do not represent child
//! data structures for the models' fields, and are entirely separate from the
//! persistent data structures.
//!
//! The data types used by events therefore do NOT need to be convertible to and
//! from database-compatible format using [`ToSql`](stash::exports::ToSql) and
//! [`FromSql`](stash::exports::FromSql). If anything in this module implements
//! those traits, it is a sign that a mistake has been made. Additionally, they
//! do not generally need to be serializable or deserializable, as they are not
//! used for network communication or any other interchange purpose as a general
//! requirement, and so implementation of [`Serialize`](serde::Serialize) and
//! [`Deserialize`](serde::Deserialize) is not necessary and may also be a sign
//! of a mistake.
//!
//! Generally speaking, [`From`] conversions to convert from the Proton API
//! types to the internal types are provided, but not vice versa unless there is
//! a specific need. Such conversions are usually very simple and indeed in many
//! cases can be done without altering any data in memory.
//!
//! This separation does cause some duplication, but the overlap is not total.
//! The various implementations for the types differ in each place; any logic
//! for the application is in the application types and not the API types; and
//! the distinction allows customisation of how the application deals with its
//! related data. Additionally, it promotes wider usability, as each application
//! that depends upon the API types can interpret and managed them in its own
//! way.
//!

use crate::datatypes::RemoteId;
use crate::models::{Contact, ContactEmail};
use proton_api_core::services::proton::response_data::{
    Action as ApiAction, ContactEmailEvent as ApiContactEmailEvent, ContactEvent as ApiContactEvent,
};
use proton_event_loop::Event;

/// TODO: Document this enum.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(u8)]
pub enum Action {
    /// TODO: Document this field.
    Delete = 0,

    /// TODO: Document this field.
    Create = 1,

    /// TODO: Document this field.
    Update = 2,

    /// TODO: Document this field.
    UpdateFlags = 3,
}

impl From<ApiAction> for Action {
    fn from(value: ApiAction) -> Self {
        match value {
            ApiAction::Delete => Self::Delete,
            ApiAction::Create => Self::Create,
            ApiAction::Update => Self::Update,
            ApiAction::UpdateFlags => Self::UpdateFlags,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactEmailEvent {
    /// TODO: Document this field.
    pub remote_id: RemoteId,

    /// TODO: Document this field.
    pub event_id: RemoteId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub contact_email: Option<ContactEmail>,

    /// TODO: Document this field.
    pub has_more: bool,
}

impl Event for ContactEmailEvent {
    type Id = RemoteId;
    type Response = ApiContactEmailEvent;

    fn event_id(&self) -> &Self::Id {
        &self.event_id
    }

    fn has_more(&self) -> bool {
        self.has_more
    }
}

impl From<ApiContactEmailEvent> for ContactEmailEvent {
    fn from(value: ApiContactEmailEvent) -> Self {
        Self {
            remote_id: value.id.into(),
            event_id: value.event_id.into(),
            action: value.action.into(),
            contact_email: value.contact_email.map(ContactEmail::from),
            has_more: value.has_more,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactEvent {
    /// TODO: Document this field.
    pub remote_id: RemoteId,

    /// TODO: Document this field.
    pub event_id: RemoteId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub contact: Option<Contact>,

    /// TODO: Document this field.
    pub has_more: bool,
}

impl Event for ContactEvent {
    type Id = RemoteId;
    type Response = ApiContactEvent;

    fn event_id(&self) -> &Self::Id {
        &self.event_id
    }

    fn has_more(&self) -> bool {
        self.has_more
    }
}

impl From<ApiContactEvent> for ContactEvent {
    fn from(value: ApiContactEvent) -> Self {
        Self {
            remote_id: value.id.into(),
            event_id: value.event_id.into(),
            action: value.action.into(),
            contact: value.contact.map(Contact::from),
            has_more: value.has_more,
        }
    }
}
