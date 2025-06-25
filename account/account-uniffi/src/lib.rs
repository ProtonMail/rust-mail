#[macro_use]
extern crate uniffi_macros;

pub mod common;
pub mod errors;
pub mod login;
pub mod password_validator;
pub mod signup;
pub mod user_behavior;

uniffi::setup_scaffolding!();
