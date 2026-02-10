//! This crate provides the backbone to executes actions optimistically on a local data set and
//! then apply them on a remote server.
//!
//! To achieve this each action needs to implement the [`Action`] and the [`Handler`] trait.
//!
//! Actions can then be executed immediately or queued for future execution on a [`Queue`] with
//! default or custom [`Metadata`].
//!
//! The [`Metadata`] allows the use to override priority, delay execution, assign dependency
//! chains, among others.
//!
//! Actions that are stored in the queue are serialized and versioned. A [`VersionConverter`] can
//! be assigned to each action to update the serialized data from previous versions.
//!
//! Finally, all actions need to be registered with [`Factory`] so they can be deserialized
//! from the queue.
//!
//! [`Action`]: action::Action
//! [`VersionConverter`]: action::VersionConverter
//! [`Metadata`]: action::Metadata
//! [`Handler`]: action::Handler
//! [`Queue`]: queue::Queue
//! [`Factory`]: action::Factory

pub mod action;
pub mod db;
pub mod observers;
pub mod queue;
pub mod rebase;

#[cfg(any(test, debug_assertions, feature = "test-utils"))]
pub mod tests {
    #[path = "../tests/common.rs"]
    pub mod common;
}
