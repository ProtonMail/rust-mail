use crate::UserContext;
use crate::models::{ModelExtension, UserSettings};
use proton_issue_reporter_service::{IssueLevel, IssueReportKeys, UserIssueReporter};
use std::sync::{Arc, Weak};
use tracing::{debug, warn};

/// We can't use the regular reporter, since we need to check if the user allows
/// telemetry reports.
pub struct UserIssueReporterService {
    ctx: Weak<UserContext>,
    reporter: Arc<dyn UserIssueReporter>,
}

impl UserIssueReporterService {
    pub fn new(ctx: Weak<UserContext>, reporter: Arc<dyn UserIssueReporter>) -> Self {
        UserIssueReporterService { ctx, reporter }
    }

    pub fn report(&self, level: IssueLevel, message: String, keys: IssueReportKeys) {
        let Some(ctx) = self.ctx.upgrade() else {
            warn!("Reporting issue, but context is dead");
            return;
        };

        let ctx_cloned = ctx.clone();
        let reporter = self.reporter.clone();
        ctx_cloned.spawn(async move {
            Self::do_report(ctx, reporter, level, message, keys).await;
        });
    }

    #[cfg(feature = "test-utils")]
    pub async fn report_inplace(&self, level: IssueLevel, message: String, keys: IssueReportKeys) {
        let Some(ctx) = self.ctx.upgrade() else {
            warn!("Reporting issue, but context is dead");
            return;
        };

        Self::do_report(ctx, self.reporter.clone(), level, message, keys).await;
    }

    async fn do_report(
        ctx: Arc<UserContext>,
        reporter: Arc<dyn UserIssueReporter>,
        level: IssueLevel,
        message: String,
        mut keys: IssueReportKeys,
    ) {
        let Ok(tether) = ctx.user_stash.connection().await else {
            warn!("Issue not reported, failed to acquire db connection");
            return;
        };

        let should_report = match UserSettings::find_by_id(ctx.user_id().clone(), &tether).await {
            Ok(Some(settings)) => settings.telemetry,
            Ok(None) => {
                warn!("User setting not found, issue will be reported");
                keys.insert(
                    "UserSettingsMissing".into(),
                    "Could not find settings".into(),
                );
                true
            }
            Err(err) => {
                warn!("Failed to load user setting, issue will be reported");
                keys.insert("UserSettingsLoadError".into(), err.to_string());
                true
            }
        };

        if !should_report {
            debug!("Issue not reported due to disabled telemetry");
            return;
        }

        let _ = tokio::task::spawn_blocking(move || {
            reporter.report(level, message, keys);
        })
        .await;
    }
}
