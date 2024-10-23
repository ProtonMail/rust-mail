#![allow(non_snake_case)]

#[cfg(any(test, debug_assertions))]
pub mod account;

#[cfg(any(test, debug_assertions))]
pub mod addresses;

#[cfg(any(test, debug_assertions))]
pub mod attachment;

#[cfg(any(test, debug_assertions))]
pub mod common;

#[cfg(any(test, debug_assertions))]
pub mod conversations;

#[cfg(any(test, debug_assertions))]
pub mod db;

#[cfg(any(test, debug_assertions))]
pub mod db_states;

#[cfg(any(test, debug_assertions))]
pub mod init;

#[cfg(any(test, debug_assertions))]
pub mod labels;

#[cfg(any(test, debug_assertions))]
pub mod message_body;

#[cfg(any(test, debug_assertions))]
pub mod messages;

#[cfg(any(test, debug_assertions))]
pub mod utils;
