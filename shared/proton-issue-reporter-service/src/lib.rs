//! This crate aims to abstract an issue reporter service like sentry.
//!
//! While we currently do not have an implementation in pure rust, this may
//! change in the future.
//!

use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum IssueLevel {
    Critical,
    Error,
    Warning,
}

pub type IssueReportKeys = HashMap<String, String>;
pub trait IssueReporter: Send + Sync {
    /// Report an issue not associate with any user.
    fn report(&self, level: IssueLevel, message: String, keys: IssueReportKeys);

    /// Create a new issue report tied to a specific user.
    fn new_user_reporter(&self, user_id: String) -> Arc<dyn UserIssueReporter>;
}

pub trait UserIssueReporter: Send + Sync {
    /// Report an issue not associate with this user.
    fn report(&self, level: IssueLevel, message: String, keys: IssueReportKeys);
}

pub struct NoopIssueReporter;
impl IssueReporter for NoopIssueReporter {
    fn report(&self, _: IssueLevel, _: String, _: IssueReportKeys) {
        //do nothing
    }
    fn new_user_reporter(&self, _: String) -> Arc<dyn UserIssueReporter> {
        Arc::new(NoopUserIssueReporter)
    }
}

pub struct NoopUserIssueReporter;

impl UserIssueReporter for NoopUserIssueReporter {
    fn report(&self, _: IssueLevel, _: String, _: IssueReportKeys) {
        //do nothing
    }
}
