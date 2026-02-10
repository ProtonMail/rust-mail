//! ## DNS
//!
//! This module provides DNS resolution capabilities for the `muon` library.
//! It defines the [`Dns`] trait, which is an abstraction over a DNS client,
//! and an adapter to use a DNS client as a [`Resolver`];
//!
//! [`Resolver`]: crate::rt::Resolver

export! {
    mod common (as pub);
    mod client (as pub);
}
