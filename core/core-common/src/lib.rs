//! Common features of the core domain, such user account and session management and per user settings.
pub mod actions;
pub mod auth_store;
mod context;
pub mod core_clock;
pub mod datatypes;
pub mod db;
pub mod device_registration;
pub mod events;
pub mod migration_snooper;
pub mod models;
pub mod observability;
pub mod os;
pub mod pin_code;
pub mod post_login_check;
mod user_context;
pub mod utils;
pub mod watch_handle;

pub mod app_events;

pub mod device;
#[cfg(test)]
mod tests;
pub mod validation;

#[cfg(feature = "test-utils")]
pub mod test_utils;

pub use context::*;
pub use user_context::*;

pub mod services {
    pub use crate::context::services::*;
    pub use crate::user_context::services::*;
}
