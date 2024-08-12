//! Event data types for the Proton Mail common library.
//!
//! This module contains various data types used by the Proton Mail common
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

#[cfg(test)]
#[path = "tests/events.rs"]
mod tests;

use crate::datatypes::{ConversationCount, MessageCount};
use crate::models::{Conversation, Label, MailSettings};
use crate::AppError;
use proton_api_mail::services::proton::response_data::{
    ConversationEvent as ApiConversationEvent, LabelEvent as ApiLabelEvent,
    MailEvent as ApiMailEvent, MessageEvent as ApiMessageEvent, MessageMetadata,
};
use proton_core_common::datatypes::{ProductUsedSpace, RemoteId};
use proton_core_common::events::{Action, ContactEmailEvent, ContactEvent};
use proton_core_common::models::{Address, User, UserSettings};
use proton_event_loop::Event;

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationEvent {
    /// TODO: Document this field.
    pub remote_id: RemoteId,

    /// TODO: Document this field.
    pub event_id: RemoteId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub conversation: Option<Conversation>,

    /// TODO: Document this field.
    pub has_more: bool,
}

impl Event for ConversationEvent {
    type Id = RemoteId;
    type Response = ApiConversationEvent;

    fn event_id(&self) -> &Self::Id {
        &self.event_id
    }

    fn has_more(&self) -> bool {
        self.has_more
    }
}

impl TryFrom<ApiConversationEvent> for ConversationEvent {
    type Error = AppError;

    fn try_from(value: ApiConversationEvent) -> Result<Self, Self::Error> {
        Ok(Self {
            remote_id: value.id.into(),
            event_id: value.event_id.into(),
            action: value.action.into(),
            conversation: value.conversation.map(Conversation::try_from).transpose()?,
            has_more: value.has_more,
        })
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LabelEvent {
    /// TODO: Document this field.
    pub remote_id: RemoteId,

    /// TODO: Document this field.
    pub event_id: RemoteId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub has_more: bool,

    /// TODO: Document this field.
    pub label: Option<Label>,
}

impl Event for LabelEvent {
    type Id = RemoteId;
    type Response = ApiLabelEvent;

    fn event_id(&self) -> &Self::Id {
        &self.event_id
    }

    fn has_more(&self) -> bool {
        self.has_more
    }
}

impl From<ApiLabelEvent> for LabelEvent {
    fn from(value: ApiLabelEvent) -> Self {
        Self {
            remote_id: value.id.into(),
            event_id: value.event_id.into(),
            action: value.action.into(),
            has_more: value.has_more,
            label: value.label.map(Label::from),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailEvent {
    /// TODO: Document this field.
    pub remote_id: RemoteId,

    /// TODO: Document this field.
    pub event_id: RemoteId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub addresses: Option<Vec<Address>>,

    /// TODO: Document this field.
    pub conversation_counts: Option<Vec<ConversationCount>>,

    /// TODO: Document this field.
    pub conversations: Option<Vec<ConversationEvent>>,

    /// TODO: Document this field.
    pub has_more: bool,

    /// TODO: Document this field.
    pub labels: Option<Vec<LabelEvent>>,

    /// TODO: Document this field.
    pub mail_settings: Option<MailSettings>,

    /// TODO: Document this field.
    pub message_counts: Option<Vec<MessageCount>>,

    /// TODO: Document this field.
    pub messages: Option<Vec<MessageEvent>>,

    /// TODO: Document this field.
    pub product_used_space: Option<ProductUsedSpace>,

    /// TODO: Document this field.
    pub used_space: Option<i64>,

    /// TODO: Document this field.
    pub user: Option<User>,

    /// TODO: Document this field.
    pub user_settings: Option<UserSettings>,
}

impl MailEvent {
    pub fn get_core_event_user(&self) -> Option<&User> {
        self.user.as_ref()
    }
    pub fn get_core_event_user_mut(&mut self) -> Option<&mut User> {
        self.user.as_mut()
    }

    pub fn get_core_event_user_settings(&self) -> Option<&UserSettings> {
        self.user_settings.as_ref()
    }
    pub fn get_core_event_user_settings_mut(&mut self) -> Option<&mut UserSettings> {
        self.user_settings.as_mut()
    }

    pub fn get_core_event_used_space(&self) -> Option<i64> {
        self.used_space
    }

    pub fn get_core_event_used_product_space(&self) -> Option<&ProductUsedSpace> {
        self.product_used_space.as_ref()
    }

    pub fn get_core_event_addresses(&self) -> Option<&[Address]> {
        self.addresses.as_deref()
    }
    pub fn get_core_event_addresses_mut(&mut self) -> Option<&mut [Address]> {
        self.addresses.as_deref_mut()
    }

    pub fn get_core_event_contacts(&self) -> Option<&[ContactEvent]> {
        unimplemented!()
    }
    pub fn get_core_event_contacts_mut(&mut self) -> Option<&mut [ContactEvent]> {
        unimplemented!()
    }

    pub fn get_core_event_contact_emails(&self) -> Option<&[ContactEmailEvent]> {
        unimplemented!()
    }
    pub fn get_core_event_contact_emails_mut(&mut self) -> Option<&mut [ContactEmailEvent]> {
        unimplemented!()
    }
}

impl Event for MailEvent {
    type Id = RemoteId;
    type Response = ApiMailEvent;

    fn event_id(&self) -> &Self::Id {
        &self.event_id
    }

    fn has_more(&self) -> bool {
        self.has_more
    }
}

impl From<ApiMailEvent> for MailEvent {
    fn from(value: ApiMailEvent) -> Self {
        let conversations = value.conversations.map(|conversations| {
            conversations
                .into_iter()
                .map(ConversationEvent::try_from)
                .filter_map(Result::ok)
                .collect()
        });

        Self {
            remote_id: value.id.into(),
            event_id: value.event_id.into(),
            action: value.action.into(),
            addresses: value
                .addresses
                .map(|addresses| addresses.into_iter().map(Address::from).collect()),
            conversation_counts: value.conversation_counts.map(|conversation_counts| {
                conversation_counts
                    .into_iter()
                    .map(ConversationCount::from)
                    .collect()
            }),
            conversations,
            has_more: value.has_more,
            labels: value
                .labels
                .map(|labels| labels.into_iter().map(LabelEvent::from).collect()),
            mail_settings: value.mail_settings.map(MailSettings::from),
            message_counts: value
                .message_counts
                .map(|message_counts| message_counts.into_iter().map(MessageCount::from).collect()),
            messages: value
                .messages
                .map(|messages| messages.into_iter().map(MessageEvent::from).collect()),
            product_used_space: value.product_used_space.map(ProductUsedSpace::from),
            used_space: value.used_space,
            user: value.user.map(User::from),
            user_settings: value.user_settings.map(UserSettings::from),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageEvent {
    /// TODO: Document this field.
    pub remote_id: RemoteId,

    /// TODO: Document this field.
    pub event_id: RemoteId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub has_more: bool,

    /// TODO: Document this field.
    pub message: Option<MessageMetadata>,
}

impl Event for MessageEvent {
    type Id = RemoteId;
    type Response = ApiMessageEvent;

    fn event_id(&self) -> &Self::Id {
        &self.event_id
    }

    fn has_more(&self) -> bool {
        self.has_more
    }
}

impl From<ApiMessageEvent> for MessageEvent {
    fn from(value: ApiMessageEvent) -> Self {
        Self {
            remote_id: value.id.into(),
            event_id: value.event_id.into(),
            action: value.action.into(),
            has_more: value.has_more,
            message: value.message.map(MessageMetadata::from),
        }
    }
}
