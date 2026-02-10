//! Core related database for user accounts, sessions and info.
//!
//! The module provide 2 distinct connection types which can be used interchangeably. It is up
//! to the user of this crate to decide whether they wish to store the user info in the same
//! or separate databases.

pub mod account;
mod addresses;
mod contacts;
mod core;
pub mod migrations;

pub type ChangeSender<T> =
    flume::Sender<stash::orm::ResultsetChange<T, <T as stash::orm::Model>::IdType>>;

pub type ChangeReceiver<T> =
    flume::Receiver<stash::orm::ResultsetChange<T, <T as stash::orm::Model>::IdType>>;

pub use proton_sqlite3;
