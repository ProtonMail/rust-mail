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

use crate::datatypes::{ConversationLabelsCount, MessageLabelsCount};
use crate::models::{Conversation, MailSettings};
use proton_core_api::services::proton::LabelEvent as ApiLabelEvent;
use proton_core_api::services::proton::{EventId, LabelId};
use proton_core_common::CoreEvent;
use proton_core_common::datatypes::ProductUsedSpace;
use proton_core_common::events::ContactEvent;
use proton_core_common::events::{Action, AddressEvent, ContactEmailEvent};
use proton_core_common::models::{Label, User, UserSettings};
use proton_core_common::utils::MapVec as _;
use proton_event_loop::Event;
use proton_mail_api::services::proton::common::{ConversationId, MessageId};
use proton_mail_api::services::proton::prelude::IncomingDefault;
use proton_mail_api::services::proton::response_data::{
    ConversationEvent as ApiConversationEvent, MailEvent as ApiMailEvent,
    MessageEvent as ApiMessageEvent, MessageMetadata,
};

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConversationEvent {
    /// TODO: Document this field.
    pub remote_id: ConversationId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub conversation: Option<Conversation>,
}

impl From<ApiConversationEvent> for ConversationEvent {
    fn from(value: ApiConversationEvent) -> Self {
        Self {
            remote_id: value.id,
            action: value.action.into(),
            conversation: value.conversation.map(Conversation::from),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LabelEvent {
    /// TODO: Document this field.
    pub remote_id: LabelId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub label: Option<Label>,
}

impl From<ApiLabelEvent> for LabelEvent {
    fn from(value: ApiLabelEvent) -> Self {
        Self {
            remote_id: value.id,
            action: value.action.into(),
            label: value.label.map(Label::from),
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MailEvent {
    /// TODO: Document this field.
    pub event_id: EventId,

    /// TODO: Document this field.
    pub addresses: Option<Vec<AddressEvent>>,

    /// TODO: Document this field.
    pub conversation_counts: Option<Vec<ConversationLabelsCount>>,

    /// TODO: Document this field.
    pub conversations: Option<Vec<ConversationEvent>>,

    /// TODO: Document this field.
    pub has_more: bool,

    pub incoming_defaults: Option<Vec<IncomingDefault>>,

    /// TODO: Document this field.
    pub labels: Option<Vec<LabelEvent>>,

    /// TODO: Document this field.
    pub mail_settings: Option<MailSettings>,

    /// TODO: Document this field.
    pub message_counts: Option<Vec<MessageLabelsCount>>,

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

    /// TODO: Document this field.
    pub contacts: Option<Vec<ContactEvent>>,

    /// TODO: Document this field.
    pub contact_emails: Option<Vec<ContactEmailEvent>>,

    /// Indicates whether we should refresh our data.
    pub refresh: u8,
}

impl CoreEvent for MailEvent {
    fn get_core_event_user(&self) -> Option<&User> {
        self.user.as_ref()
    }

    fn get_core_event_user_mut(&mut self) -> Option<&mut User> {
        self.user.as_mut()
    }

    fn get_core_event_user_settings(&self) -> Option<&UserSettings> {
        self.user_settings.as_ref()
    }

    fn get_core_event_user_settings_mut(&mut self) -> Option<&mut UserSettings> {
        self.user_settings.as_mut()
    }

    fn get_core_event_used_space(&self) -> Option<i64> {
        self.used_space
    }

    fn get_core_event_used_product_space(&self) -> Option<&ProductUsedSpace> {
        self.product_used_space.as_ref()
    }

    fn get_core_event_addresses(&self) -> Option<&[AddressEvent]> {
        self.addresses.as_deref()
    }

    fn get_core_event_addresses_mut(&mut self) -> Option<&mut [AddressEvent]> {
        self.addresses.as_deref_mut()
    }

    fn get_core_event_contacts(&self) -> Option<&[ContactEvent]> {
        self.contacts.as_deref()
    }

    fn get_core_event_contacts_mut(&mut self) -> Option<&mut [ContactEvent]> {
        self.contacts.as_deref_mut()
    }

    fn get_core_event_contact_emails(&self) -> Option<&[ContactEmailEvent]> {
        self.contact_emails.as_deref()
    }

    fn get_core_event_contact_emails_mut(&mut self) -> Option<&mut [ContactEmailEvent]> {
        self.contact_emails.as_deref_mut()
    }
}

impl Event for MailEvent {
    type Response = ApiMailEvent;

    fn event_id(&self) -> &EventId {
        &self.event_id
    }

    fn has_more(&self) -> bool {
        self.has_more
    }

    fn is_refresh(&self) -> bool {
        self.refresh != 0
    }
}

impl From<ApiMailEvent> for MailEvent {
    fn from(value: ApiMailEvent) -> Self {
        Self {
            event_id: value.event_id,
            addresses: value.addresses.map(|addresses| addresses.map_vec()),
            conversation_counts: value.conversation_counts.map(|conversation_counts| {
                conversation_counts
                    .into_iter()
                    .map(ConversationLabelsCount::from)
                    .collect()
            }),
            conversations: value.conversations.map(|conversations| {
                conversations
                    .into_iter()
                    .map(ConversationEvent::from)
                    .collect()
            }),
            has_more: value.has_more,
            labels: value.labels.map(|labels| labels.map_vec()),
            mail_settings: value.mail_settings.map(MailSettings::from),
            message_counts: value.message_counts.map(|message_counts| {
                message_counts
                    .into_iter()
                    .map(MessageLabelsCount::from)
                    .collect()
            }),
            messages: value.messages.map(|messages| messages.map_vec()),
            product_used_space: value.product_used_space.map(ProductUsedSpace::from),
            used_space: value.used_space,
            user: value.user.map(User::from),
            user_settings: value.user_settings.map(UserSettings::from),
            contacts: value.contacts.map(|contacts| contacts.map_vec()),
            contact_emails: value.contact_emails.map(|contact_emails| {
                contact_emails
                    .into_iter()
                    .map(ContactEmailEvent::from)
                    .collect()
            }),
            refresh: value.refresh,
            incoming_defaults: value.incoming_defaults,
        }
    }
}

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessageEvent {
    /// TODO: Document this field.
    pub remote_id: MessageId,

    /// TODO: Document this field.
    pub action: Action,

    /// TODO: Document this field.
    pub message: Option<MessageMetadata>,
}

impl From<ApiMessageEvent> for MessageEvent {
    fn from(value: ApiMessageEvent) -> Self {
        Self {
            remote_id: value.id,
            action: value.action.into(),
            message: value.message,
        }
    }
}
