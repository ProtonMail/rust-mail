use std::time::Duration;

use crate::services::proton::muon::http::HttpReq;
use muon::common::RetryPolicy;

pub trait HttpReqExt: Sized {
    #[must_use]
    fn with_allowed_time(self, allowed_time: Option<Duration>) -> Self;
    #[must_use]
    fn with_retry_policy(self, policy: Option<RetryPolicy>) -> Self;
}

impl HttpReqExt for HttpReq {
    fn with_allowed_time(self, allowed_time: Option<Duration>) -> Self {
        if let Some(allowed_time) = allowed_time {
            return self.allowed_time(allowed_time);
        }
        self
    }

    fn with_retry_policy(self, policy: Option<RetryPolicy>) -> Self {
        if let Some(policy) = policy {
            return self.retry_policy(policy);
        }
        self
    }
}
