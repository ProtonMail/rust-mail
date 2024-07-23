//! Uniffi Bindings for everything related to mail.
//!
//! # Getting Started
//!
//! An application is expected to initialize a `MailContext` which needs to be kept alive
//! for the lifetime of the application.
//!
//! Next a [`MailUserContext`] needs to be created in other to access all the user settings and
//! labels ([`Mailbox`]). You can obtain one by performing a login of a new user with
//! [`mail::MailSession::new_login_flow`] or by using an existing session with
//! [`mail::MailSession::user_context_from_session`]. You now have access to all the labels and
//! user related settings.
//!
//! Finally, to access the conversations you need to create a [`Mailbox`] for the active label.
//! Once a mailbox has been created you need to create a live query for the conversation of that
//! mailbox with [`mail::Mailbox::new_conversation_live_query`].
//!
//! [MailContext]: mail::MailSession
//! [MailBox]: mail::Mailbox
//! [MailUserContext]: mail::MailUserSession
//!
//! # Actions
//!
//! Mutable changes to the domain will all generate actions that are queue for execution a time that
//! makes sense for the client.
//! To execute any pending actions call [`mail::MailUserSession::execute_pending_action`] to execute one action
//! or [`mail::MailUserSession::execute_pending_actions`] to execute all pending actions.
//!

pub mod core;
mod log;
mod macros;
pub mod mail;
pub mod message_detector;

uniffi::setup_scaffolding!();
