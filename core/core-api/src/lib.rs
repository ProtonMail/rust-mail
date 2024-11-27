//! Rust bindings for the REST API for Proton

pub mod auth;
pub mod consts;
pub mod crypto_clock;
pub mod login;
pub mod service;
pub mod services;
pub mod session;

pub const MAX_PAGE_ELEMENT_COUNT: usize = 200;
pub const SYNC_CONTACT_PAGE_SIZE: usize = 1000;

pub const DEFAULT_APP_VERSION: &str = "Other";
pub const DEFAULT_CLIENT: &str = "NoClient/0.1.0";
pub const DEFAULT_HOST_URL: &str = "https://mail.proton.me/api/";
pub const DEFAULT_REDIRECT_URL: &str = "https://protonmail.ch/";

#[allow(unused)] // it is used by the http implementations
pub(crate) const X_PM_APP_VERSION_HEADER: &str = "X-Pm-Appversion";
pub(crate) const X_PM_UID_HEADER: &str = "X-Pm-Uid";
pub(crate) const X_PM_HUMAN_VERIFICATION_TOKEN: &str = "X-Pm-Human-Verification-Token";
pub(crate) const X_PM_HUMAN_VERIFICATION_TOKEN_TYPE: &str = "X-Pm-Human-Verification-Token-Type";
