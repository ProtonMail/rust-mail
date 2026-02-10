use crate::datatypes::{ProductUsedSpace, Refresh};
use crate::models::{Address, Contact, ContactEmail, Label, User, UserSettings};
use crate::utils::MapVec;
use proton_core_api::services::proton::{
    Action as ApiAction, AddressEvent as ApiAddressEvent,
    ContactEmailEvent as ApiContactEmailEvent, ContactEvent as ApiContactEvent,
    CoreEvent as ApiCoreEvent, EventId, LabelEvent as ApiLabelEvent, LabelId, ProtonIdMarker,
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
    pub async fn log_entry<T: ProtonIdMarker>(
        self,
        id: &T,
        local_id: impl AsyncFnOnce(&T) -> Option<u64>,
    ) {
        let action_str = match self {
            Action::Delete => "Deleting",
            Action::Create => "Creating",
            Action::Update => "Updating",
            Action::UpdateFlags => "Updating (flags)",
        };

        if self != Action::Create
            && let Some(local_id) = local_id(id).await
        {
            tracing::info!("{action_str} {id:?} -> {local_id}");
        } else {
            tracing::info!("{action_str} {id:?}");
        }
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

/// An event related to a [`ContactEmail`] record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactEmailEvent {
    /// The remote ID of the contact email.
    pub remote_id: ContactEmailId,

    /// The action that was taken on the contact email.
    pub action: Action,

    /// The contact email metadata.
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

/// An event related to a [`Contact`] record.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ContactEvent {
    /// The remote ID of the contact.
    pub remote_id: ContactId,

    /// The action that was taken on the contact.
    pub action: Action,

    /// The contact metadata.
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LabelEvent {
    pub remote_id: LabelId,

    pub action: Action,

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

/// Core event data structure that contains only the core fields from events.
/// This is identical to `MailEvent` but contains only the core-related fields.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CoreEvent {
    /// The event unique ID.
    pub event_id: EventId,

    /// The addresses events.
    pub addresses: Option<Vec<AddressEvent>>,

    /// Whether there are more events to fetch.
    pub has_more: bool,

    /// The product used space (for example the space used for user's mail).
    pub product_used_space: Option<ProductUsedSpace>,

    /// The used space (amount of space used by the user as a whole).
    pub used_space: Option<i64>,

    /// The user data events.
    pub user: Option<User>,

    /// The user settings events.
    pub user_settings: Option<UserSettings>,

    /// The contacts events.
    pub contacts: Option<Vec<ContactEvent>>,

    pub labels: Option<Vec<LabelEvent>>,

    /// Indicates whether we should refresh our data.
    pub refresh: Refresh,
}

impl From<ApiCoreEvent> for CoreEvent {
    fn from(value: ApiCoreEvent) -> Self {
        Self {
            event_id: value.event_id,
            addresses: value.addresses.map(MapVec::map_vec),
            has_more: value.has_more,
            product_used_space: value.product_used_space.map(ProductUsedSpace::from),
            used_space: value.used_space,
            user: value.user.map(User::from),
            user_settings: value.user_settings.map(UserSettings::from),
            contacts: value.contacts.map(MapVec::map_vec),
            refresh: value.refresh.into(),
            labels: value.labels.map(MapVec::map_vec),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Default for CoreEvent {
    fn default() -> Self {
        Self {
            event_id: EventId::from("default"),
            addresses: None,
            has_more: false,
            product_used_space: None,
            used_space: None,
            user: None,
            user_settings: None,
            contacts: None,
            refresh: Refresh::None,
            labels: None,
        }
    }
}
