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

use crate::models::{Address, Contact, ContactEmail};
use proton_core_api::services::proton::{
    Action as ApiAction, AddressEvent as ApiAddressEvent,
    ContactEmailEvent as ApiContactEmailEvent, ContactEvent as ApiContactEvent, ProtonIdMarker,
};
use proton_core_api::services::proton::{AddressId, ContactEmailId, ContactId};

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

impl Action {
    pub fn log_entry(&self, id: &impl ProtonIdMarker) {
        let action_str = match self {
            Action::Delete => "Deleting",
            Action::Create => "Creating",
            Action::Update => "Updating",
            Action::UpdateFlags => "Updating (flags)",
        };
        tracing::info!("{action_str} {id:?}");
    }
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
    pub remote_id: ContactEmailId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub contact_email: Option<ContactEmail>,
}

impl From<ApiContactEmailEvent> for ContactEmailEvent {
    fn from(value: ApiContactEmailEvent) -> Self {
        Self {
            remote_id: value.id,
            action: value.action.into(),
            contact_email: value.contact_email.map(ContactEmail::from),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactEvent {
    pub remote_id: ContactId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub contact: Option<Contact>,
}

impl From<ApiContactEvent> for ContactEvent {
    fn from(value: ApiContactEvent) -> Self {
        Self {
            remote_id: value.id,
            action: value.action.into(),
            contact: value.contact.map(Contact::from),
        }
    }
}

/// An address event.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AddressEvent {
    /// The remote ID of the address.
    pub remote_id: AddressId,

    /// The action that was taken on the address.
    pub action: Action,

    /// The address metadata.
    pub address: Option<Address>,
}

impl From<ApiAddressEvent> for AddressEvent {
    fn from(value: ApiAddressEvent) -> Self {
        Self {
            remote_id: value.id,
            action: value.action.into(),
            address: value.address.map(Address::from),
        }
    }
}
