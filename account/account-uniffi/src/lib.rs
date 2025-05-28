#[macro_use]
extern crate uniffi_macros;

pub mod errors;
pub mod login;
pub mod signup;

uniffi::setup_scaffolding!();
