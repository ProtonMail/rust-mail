use async_trait::async_trait;
use proton_log_service::LogService;

use crate::CoreContextError;

use super::Service;

// Wrapper over [`LogService`] to prevent orphan rules
pub(crate) struct LoggingService {
    service: LogService,
}

impl LoggingService {
    pub(crate) fn new(service: LogService) -> Self {
        Self { service }
    }

    pub(crate) fn service(&self) -> &LogService {
        &self.service
    }
}

#[async_trait]
impl Service for LoggingService {
    type Error = CoreContextError;
}
