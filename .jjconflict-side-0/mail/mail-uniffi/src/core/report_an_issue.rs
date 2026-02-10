use std::sync::Arc;

use crate::{
    errors::{UserSessionError, VoidSessionResult},
    mail::MailUserSession,
    uniffi_async,
};

use super::datatypes::IssueReport;
use proton_core_common::datatypes::report_an_issue as real_report_an_issue;
use proton_mail_common::{MailContextError, ProtonMailError as RealProtonMailError};

#[uniffi_export]
#[returns(VoidSessionResult)]
pub async fn report_an_issue(
    session: Arc<MailUserSession>,
    issue_report: IssueReport,
) -> Result<(), UserSessionError> {
    let mail_user_ctx = session.ctx()?;
    uniffi_async(async move {
        real_report_an_issue(issue_report.into(), mail_user_ctx.user_context())
            .await
            .map_err(MailContextError::from)?;
        Result::<_, RealProtonMailError>::Ok(())
    })
    .await
    .map_err(UserSessionError::from)
    .into()
}
