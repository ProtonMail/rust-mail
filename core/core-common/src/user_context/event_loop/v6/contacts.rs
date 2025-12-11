mod event_provider;
mod event_source;
mod event_store;
mod event_subscriber;

pub use event_source::*;
pub use event_subscriber::*;

use crate::UserContext;
use anyhow::Context;
use std::sync::{Arc, Weak};

#[derive(Clone)]
pub struct ContactEventLoopV6Context(Weak<UserContext>);

impl ContactEventLoopV6Context {
    pub fn inner(&self) -> Result<Arc<UserContext>, anyhow::Error> {
        self.0.upgrade().context("UserContext no longer alive")
    }

    #[must_use]
    pub fn boxed(&self) -> Box<Self> {
        Box::new(self.clone())
    }
}
impl From<Weak<UserContext>> for ContactEventLoopV6Context {
    fn from(value: Weak<UserContext>) -> Self {
        Self(value)
    }
}
