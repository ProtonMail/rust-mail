use crate::MailUserContext;
use anyhow::Context;
use std::sync::{Arc, Weak};

mod event_provider;
mod event_source;
mod event_store;
mod event_subscriber;

pub use event_source::*;
pub use event_subscriber::*;

#[derive(Clone)]
pub struct MailEventLoopV6Context(Weak<MailUserContext>);

impl MailEventLoopV6Context {
    pub fn inner(&self) -> Result<Arc<MailUserContext>, anyhow::Error> {
        self.0.upgrade().context("UserContext no longer alive")
    }

    #[must_use]
    pub fn boxed(&self) -> Box<Self> {
        Box::new(self.clone())
    }
}

impl From<Weak<MailUserContext>> for MailEventLoopV6Context {
    fn from(value: Weak<MailUserContext>) -> Self {
        Self(value)
    }
}
