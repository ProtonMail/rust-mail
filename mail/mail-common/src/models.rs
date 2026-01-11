//! Models for the Proton Mail common library.
//!
//! This module contains the models used by the Proton Mail common library.
//! Models are data structures that can be saved in the database, and are used
//! to represent usable persistent data throughout the application. They are
//! distinctly different from any comparative structures used when interfacing
//! with the Proton API, which are used to represent data in transit only.
//!
//! Notably, the types in this module need to have [`Model`] applied, as they
//! should represent a record in a database table. All of their fields need to
//! be convertible to and from database-compatible format using [`ToSql`](stash::exports::ToSql)
//! and [`FromSql`](stash::exports::FromSql). They do not generally need to be
//! serializable or deserializable, as they are not used for network
//! communication or any other interchange purpose as a general requirement, and
//! so implementation of [`Serialize`](serde::Serialize) and [`Deserialize`](serde::Deserialize)
//! is not necessary and may be a sign of a mistake. The exception here is for
//! child types, used by the models, for which these [`serde`] conversions are
//! desirable to lean on in order to provide conversion to and from SQL types,
//! for instance using [`sql_using_serde`](stash::utils::sql_using_serde), as a
//! convenience mechanism. This is notably useful when wanting to store types as
//! JSON in a database field, for instance. However, child types should be
//! placed into the [`datatypes`](crate::datatypes) module, with only
//! first-order models being placed into this module.
//!
//! Generally speaking, [`From`] conversions to convert from the Proton API
//! types to the internal types are provided, but not vice versa unless there is
//! a specific need.
//!
mod attachment;
pub mod attachment_cache;
mod busy_label;
mod conversation;
mod custom_settings;
mod draft;
mod incoming_default;
mod labels_with_counters;
mod mail_scroller;
mod mail_settings;
mod mailbox_labels;
mod message;
mod message_tracker;
mod message_tracker_url;
mod message_utm_link;
mod message_utm_link_url;
mod network;
mod rollback_item;

pub use self::attachment::*;
pub use self::busy_label::*;
pub use self::conversation::*;
pub use self::custom_settings::*;
pub use self::draft::*;
pub use self::incoming_default::*;
pub use self::labels_with_counters::*;
pub use self::mail_scroller::*;
pub use self::mail_settings::*;
pub use self::mailbox_labels::*;
pub use self::message::*;
pub use self::message_tracker::*;
pub use self::message_tracker_url::*;
pub use self::message_utm_link::*;
pub use self::message_utm_link_url::*;
pub use self::network::*;
pub use self::rollback_item::*;
