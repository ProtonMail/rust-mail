use crate::CoreContextError;
use crate::services::Service;
use async_trait::async_trait;
use proton_issue_reporter_service::IssueReporter;
use std::sync::Arc;

pub struct IssueReporterService {
    reporter: Arc<dyn IssueReporter>,
}

impl IssueReporterService {
    pub fn new(reporter: Arc<dyn IssueReporter>) -> Self {
        Self { reporter }
    }

    #[must_use]
    pub fn reporter(&self) -> &dyn IssueReporter {
        self.reporter.as_ref()
    }

    #[must_use]
    pub fn reporter_arc(&self) -> Arc<dyn IssueReporter> {
        self.reporter.clone()
    }
}

#[async_trait]
impl Service for IssueReporterService {
    type Error = CoreContextError;
}
