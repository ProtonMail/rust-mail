use crate::core::datatypes::Id;
use crate::errors::UserSessionError;
use crate::mail::datatypes::TrackerInfo;
use crate::mail::user_session::MailUserSession;
use crate::uniffi_async;
use proton_mail_common::ProtonMailError as RealProtonMailError;
use proton_mail_common::TrackerDetector;

#[uniffi_export]
pub async fn get_tracker_info_for_message(
    session: &MailUserSession,
    message_id: Id,
) -> Result<TrackerInfo, UserSessionError> {
    let stash = session.user_stash()?;

    uniffi_async::<_, RealProtonMailError, _>(async move {
        let tether = stash.connection().await?;
        Ok(
            TrackerDetector::get_tracker_info(message_id.into(), &tether)
                .await?
                .into(),
        )
    })
    .await
    .map_err(UserSessionError::from)
}
