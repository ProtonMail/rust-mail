use serde_repr::{Deserialize_repr, Serialize_repr};

crate::utils::string_id!(EventId);

#[derive(Debug, Serialize_repr, Deserialize_repr, Eq, PartialEq, Copy, Clone)]
#[repr(u8)]
pub enum MoreEvents {
    No = 0,
    Yes = 1,
}
#[derive(Debug, Serialize_repr, Deserialize_repr, Eq, PartialEq, Copy, Clone)]
#[repr(u8)]
pub enum EventAction {
    Delete = 0,
    Create = 1,
    Update = 2,
    UpdateFlags = 3,
}

/// Marker to indicate that that the type is a valid event type.
pub trait IsEvent: for<'de> serde::Deserialize<'de> + serde::Serialize {}
#[macro_export]
macro_rules! declare_event {
    ($name:ident, {$($member_name:ident : $member_type:ty),+}) => {
        #[derive(Debug, serde::Serialize, serde::Deserialize, Clone, Eq, PartialEq)]
        #[serde(rename_all ="PascalCase")]
        pub struct $name {
            #[serde(rename = "EventID")]
            pub event_id: $crate::domain::EventId,
            pub more: $crate::domain::MoreEvents,
            $(pub $member_name: $member_type,)+
        }

        impl $crate::domain::IsEvent for $name {}
    };
}

#[test]
fn test_custom_event_type() {
    // compile test for this macro
    declare_event!(MyEvent, {foo:i32, bar:bool, zeta:String});
    declare_event!(MySingleEvent, {foo:i32});

    let _ = MyEvent {
        event_id: EventId::from("test_id"),
        more: MoreEvents::No,
        foo: 32,
        bar: false,
        zeta: "".to_string(),
    };

    let _ = MySingleEvent {
        event_id: EventId::from("bar"),
        more: MoreEvents::No,
        foo: 0,
    };
}
