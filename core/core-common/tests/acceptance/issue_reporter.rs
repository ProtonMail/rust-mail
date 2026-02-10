use proton_core_common::datatypes::{
    DateFormat, Density, LogAuth, NotificationSettings, WeekStart,
};
use proton_core_common::models::UserSettings;
use proton_core_common::test_utils::test_context::TestContext;
use proton_issue_reporter_service::{
    IssueLevel, IssueReportKeys, IssueReporter, UserIssueReporter,
};
use stash::orm::Model;
use std::sync::Arc;

#[tokio::test]
async fn issues_not_report_if_telemetry_disabled() {
    let ctx = TestContext::with_issue_reporter(Arc::new(PanicReporter)).await;
    let user_ctx = ctx.user_context().await;

    #[allow(clippy::default_trait_access)]
    let mut settings = UserSettings {
        remote_id: Some(user_ctx.user_id().clone()),
        crash_reports: false,
        date_format: DateFormat::Default,
        density: Density::Comfortable,
        device_recovery: false,
        early_access: false,
        email: Default::default(),
        flags: Default::default(),
        hide_side_panel: false,
        high_security: Default::default(),
        invoice_text: String::new(),
        locale: String::new(),
        log_auth: LogAuth::Disabled,
        news: NotificationSettings::default(),
        password: Default::default(),
        phone: Default::default(),
        referral: None,
        session_account_recovery: false,
        telemetry: false,
        time_format: Default::default(),
        two_factor_auth: Default::default(),
        week_start: WeekStart::Default,
        welcome: false,
    };

    user_ctx
        .stash()
        .connection()
        .await
        .unwrap()
        .tx(async |tx| settings.save(tx).await)
        .await
        .unwrap();

    let user_reporter = user_ctx.issue_reporter_service();
    user_reporter
        .report_inplace(
            IssueLevel::Critical,
            "Something went wrong".into(),
            IssueReportKeys::default(),
        )
        .await;
}

struct PanicReporter;

impl IssueReporter for PanicReporter {
    fn report(&self, _: IssueLevel, _: String, _: IssueReportKeys) {
        panic!("Issue reported when it shouldn't");
    }

    fn new_user_reporter(&self, _: String) -> Arc<dyn UserIssueReporter> {
        Arc::new(PanicReporter)
    }
}

impl UserIssueReporter for PanicReporter {
    fn report(&self, _: IssueLevel, _: String, _: IssueReportKeys) {
        panic!("Issue reported when it shouldn't");
    }
}
