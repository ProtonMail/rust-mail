//! V6 Compatible event loop manager.
//!
//! Contrary to v5 or earlier versions, the v6 event loop only passes a limited set of information
//! back to the clients and expects them to fetch the data themselves as needed.
//!
//! Another import changes in v6 is that domains are now have their own event loop. E.g.: in v5,
//! Core, Mail and other domains all shared the same endpoint. in V6 there are dedicated endpoints
//! for Core and Mail.
//!
//! We also have to guarantee that Core event loop runs before other event loops to guarantee some
//! consistency between the flow of information. While we can guarantee ordering of event polls,
//! it is still possible for one event poll to pull in information ahead of time. It's recommended
//! that subscribers fetch missing information and not relly on the Core loop to provide it for them.
//!
//! # Example
//!
//! <../../examples/v6.rs>
//!
//! ```
mod manager;
mod poller;
mod source;
mod subscriber;

pub use manager::*;
pub use source::*;
pub use subscriber::*;
