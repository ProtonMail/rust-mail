use serde;
use serde::{Deserialize, Serialize};
use serde_repr::{Deserialize_repr, Serialize_repr};

crate::utils::string_id!(EventId);

#[derive(Debug, Serialize_repr, Deserialize_repr, Eq, PartialEq, Copy, Clone)]
#[repr(u8)]
pub enum More {
    No = 0,
    Yes = 1,
}
#[derive(Debug, Serialize_repr, Deserialize_repr, Eq, PartialEq, Copy, Clone)]
#[repr(u8)]
pub enum Action {
    Delete = 0,
    Create = 1,
    Update = 2,
    UpdateFlags = 3,
}

/// Marker to indicate that that the type is a valid event type.
pub trait Event:
    for<'de> Deserialize<'de>
    + Serialize
    + Clone
    + Eq
    + PartialEq
    + std::fmt::Debug
    + Send
    + Sync
    + 'static
{
    fn event_id(&self) -> &EventId;

    fn has_more(&self) -> bool;
}

#[allow(clippy::module_name_repetitions)] // this macro is exported at the route of the crate
#[macro_export]
macro_rules! declare_event {
    ($name:ident, {$($member_name:ident : $member_type:ty),+}) => {
        #[derive(Debug, Serialize, Deserialize, Clone, Eq, PartialEq)]
        #[serde(crate = "self::serde", rename_all ="PascalCase")]
        pub struct $name {
            #[serde(rename = "EventID")]
            pub event_id: $crate::domain::EventId,
            pub more: $crate::domain::More,
            $(pub $member_name: $member_type,)+
        }

        impl $crate::domain::Event for $name {
            fn event_id(&self) -> &$crate::domain::EventId {
                &self.event_id
            }

            fn has_more(&self) -> bool {
                self.more == $crate::domain::More::Yes
            }
        }
    };
}

#[test]
fn test_custom_event_type() {
    // compile test for this macro
    declare_event!(MyEvent, {foo:i32, bar:bool, zeta:String});
    declare_event!(MySingleEvent, {foo:i32});

    let _ = MyEvent {
        event_id: EventId::from("test_id"),
        more: More::No,
        foo: 32,
        bar: false,
        zeta: String::new(),
    };

    let _ = MySingleEvent {
        event_id: EventId::from("bar"),
        more: More::No,
        foo: 0,
    };
}
