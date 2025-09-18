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

pub fn issue_report_keys_from_error(e: impl std::error::Error) -> IssueReportKeys {
    IssueReportKeys::from([("error".into(), format!("{e:?}"))])
}

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

pub struct TracedIssueReporter(Arc<dyn IssueReporter>);

impl TracedIssueReporter {
    pub fn new(reporter: Arc<dyn IssueReporter>) -> Self {
        TracedIssueReporter(reporter)
    }
}
pub struct TracedUserIssueReporter {
    user_id: String,
    reporter: Arc<dyn UserIssueReporter>,
}

impl IssueReporter for TracedIssueReporter {
    fn report(&self, level: IssueLevel, message: String, keys: IssueReportKeys) {
        tracing::error!(?level, "Issue Report: {message}\n keys: {keys:?}");
        self.0.report(level, message, keys);
    }

    fn new_user_reporter(&self, user_id: String) -> Arc<dyn UserIssueReporter> {
        let user_id_cloned = user_id.clone();
        Arc::new(TracedUserIssueReporter {
            user_id: user_id_cloned,
            reporter: self.0.new_user_reporter(user_id),
        })
    }
}

impl UserIssueReporter for TracedUserIssueReporter {
    fn report(&self, level: IssueLevel, message: String, keys: IssueReportKeys) {
        tracing::error!(?self.user_id, ?level,"Issue Report: {message}\n keys: {keys:?}");
        self.reporter.report(level, message, keys);
    }
}
