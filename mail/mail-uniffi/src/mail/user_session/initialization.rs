// use crate::errors::{UserSessionError, VoidSessionResult};
// use crate::mail::MailUserSession;
// use crate::uniffi_async;
// use proton_mail_common::MailUserContext;
// use proton_mail_common::errors::ProtonMailError as RealProtonMailError;

// #[uniffi_export]
// impl MailUserSession {
//     /// Initialize the user context. Should be called at least once.
//     ///
//     /// *NOTE*: You should not create any [`crate::mail::Mailbox`] types until this initialization has
//     /// completed.
//     #[returns(VoidSessionResult)]
//     pub async fn initialize(
//         &self,
//         cb: Box<dyn MailUserSessionInitializationCallback>,
//     ) -> Result<(), UserSessionError> {
//         let ctx = self.ctx()?;
//         uniffi_async(async move {
//             let cb = Box::new(FFIMailUserInitializationCallback::from(cb));
//             let cb_ref = cb.as_ref();
//             if let Err((_, e)) = MailUserContext::initialize_async(ctx, cb_ref).await {
//                 return Err(RealProtonMailError::from(e));
//             }
//             Ok(())
//         })
//         .await
//         .map_err(UserSessionError::from)
//         .into()
//     }
// }

/// Stage of the initialization that is currently being handled.
#[derive(Debug, Copy, Clone, Eq, PartialEq, uniffi::Enum)]
pub enum MailUserSessionInitializationStage {
    /// Before components started to initialize, it already failed.
    Initialization,
    User,
    MailSettings,
    Addresses,
    Events,
    Labels,
    Contacts,
    Counters,
}

impl From<proton_mail_common::MailUserContextLoadingStage> for MailUserSessionInitializationStage {
    fn from(value: proton_mail_common::MailUserContextLoadingStage) -> Self {
        match value {
            proton_mail_common::MailUserContextLoadingStage::UserSettings => Self::User,
            proton_mail_common::MailUserContextLoadingStage::MailSettings => Self::MailSettings,
            proton_mail_common::MailUserContextLoadingStage::Addresses => Self::Addresses,
            proton_mail_common::MailUserContextLoadingStage::Events => Self::Events,
            proton_mail_common::MailUserContextLoadingStage::Labels => Self::Labels,
            proton_mail_common::MailUserContextLoadingStage::Contacts => Self::Contacts,
            proton_mail_common::MailUserContextLoadingStage::Counters => Self::Counters,
            proton_mail_common::MailUserContextLoadingStage::Initialization => Self::Initialization,
        }
    }
}
