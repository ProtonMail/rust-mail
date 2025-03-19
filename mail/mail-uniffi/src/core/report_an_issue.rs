use std::sync::Arc;

use crate::{
    errors::{UserSessionError, VoidSessionResult},
    mail::MailUserSession,
};

use super::datatypes::IssueReport;

#[allow(clippy::unused_async)]
#[allow(unused_variables)]
#[uniffi_export]
#[returns(VoidSessionResult)]
pub async fn report_an_issue(
    session: Arc<MailUserSession>,
    issue_report: IssueReport,
) -> Result<(), UserSessionError> {
    Result::<_, UserSessionError>::Ok(())
}
