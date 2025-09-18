//! Uniffi bindings for the proton-issue-reporter-service

use proton_issue_reporter_service::{
    IssueLevel as RealIssueLevel, IssueReportKeys, IssueReporter as RealIssueReporter,
    UserIssueReporter as RealUserIssueReporter,
};
use std::sync::Arc;

#[derive(Debug, uniffi::Enum)]
pub enum IssueLevel {
    Critical,
    Error,
    Warning,
}

impl From<RealIssueLevel> for IssueLevel {
    fn from(level: RealIssueLevel) -> Self {
        match level {
            RealIssueLevel::Critical => IssueLevel::Critical,
            RealIssueLevel::Error => IssueLevel::Error,
            RealIssueLevel::Warning => IssueLevel::Warning,
        }
    }
}

impl From<IssueLevel> for RealIssueLevel {
    fn from(level: IssueLevel) -> Self {
        match level {
            IssueLevel::Critical => Self::Critical,
            IssueLevel::Error => Self::Error,
            IssueLevel::Warning => Self::Warning,
        }
    }
}

#[uniffi::export(with_foreign)]
pub trait IssueReporter: Sync + Send {
    fn report(&self, level: IssueLevel, message: String, keys: IssueReportKeys);

    fn new_user_reporter(&self, user_id: String) -> Arc<dyn UserIssueReporter>;
}

#[uniffi::export(with_foreign)]
pub trait UserIssueReporter: Sync + Send {
    fn report(&self, level: IssueLevel, message: String, keys: IssueReportKeys);
}

pub struct IssueReporterWrapper(Arc<dyn IssueReporter>);

impl IssueReporterWrapper {
    pub fn new(reporter: Arc<dyn IssueReporter>) -> Arc<Self> {
        Arc::new(Self(reporter))
    }
}
impl RealIssueReporter for IssueReporterWrapper {
    fn report(&self, level: RealIssueLevel, message: String, keys: IssueReportKeys) {
        self.0.report(level.into(), message, keys);
    }

    fn new_user_reporter(&self, user_id: String) -> Arc<dyn RealUserIssueReporter> {
        Arc::new(UserIssueReporterWrapper(self.0.new_user_reporter(user_id)))
    }
}

struct UserIssueReporterWrapper(Arc<dyn UserIssueReporter>);

impl RealUserIssueReporter for UserIssueReporterWrapper {
    fn report(&self, level: RealIssueLevel, message: String, keys: IssueReportKeys) {
        self.0.report(level.into(), message, keys);
    }
}

uniffi::setup_scaffolding!();
