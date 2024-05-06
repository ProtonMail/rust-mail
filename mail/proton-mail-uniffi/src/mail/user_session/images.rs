use crate::mail::{map_task_join_error, MailUserSession};
use crate::mail::{MailSessionError, MailSessionResult};
use proton_mail_common::proton_api_mail::domain::{LightOrDarkMode, MessageAddress};

#[uniffi::export]
impl MailUserSession {
    /// Get the sender image for a list of senders.
    ///
    /// # Parameters
    /// * `size`: Is used to give the x*x size of the returned image (will default to 32 if none provided).
    /// * `mode`: Can be used to select if the "light" or "dark" mode of the image is desired (default is light).
    /// * `format`: Desired image format, if none is specified the default format of the image will be used.
    ///
    /// Returns `None` if no image needs to be displayed.
    ///
    /// # Errors
    /// Returns errors if the API call fails, the mode value is invalid, the conversation doesn't exist, or
    /// if there's an issue with the sender that causes problems when creating the API request on our side.
    pub async fn image_for_senders(
        &self,
        senders: Vec<MessageAddress>,
        size: Option<u32>,
        mode: Option<String>,
        format: Option<String>,
    ) -> MailSessionResult<Option<Vec<u8>>> {
        let mode = light_or_dark_mode_from_string(mode)?;

        let ctx = self.ctx.clone();
        Ok(self
            .ctx
            .mail_context()
            .clone()
            .async_runtime()
            .spawn(async move {
                //TODO (ET-208) replace when we have saving to files or uniffi supports Bytes
                ctx.image_for_senders(&senders, size, mode, format)
                    .await
                    .map(|v| v.map(|v| v.to_vec()))
            })
            .await
            .map_err(map_task_join_error)??)
    }

    /// Get the sender image for a sender address.
    ///
    /// # Parameters
    /// * `size`: Is used to give the x*x size of the returned image (will default to 32 if none provided).
    /// * `mode`: Can be used to select if the "light" or "dark" mode of the image is desired (default is light).
    /// * `format`: Desired image format, if none is specified the default format of the image will be used.
    ///
    /// Returns `None` if no image needs to be displayed.
    ///
    /// # Errors
    /// Returns errors if the API call fails, the mode value is invalid, the conversation doesn't exist, or
    /// if there's an issue with the sender that causes problems when creating the API request on our side.
    pub async fn image_for_sender(
        &self,
        sender: MessageAddress,
        size: Option<u32>,
        mode: Option<String>,
        format: Option<String>,
    ) -> MailSessionResult<Option<Vec<u8>>> {
        let mode = light_or_dark_mode_from_string(mode)?;
        let ctx = self.ctx.clone();
        Ok(self
            .ctx
            .mail_context()
            .clone()
            .async_runtime()
            .spawn(async move {
                //TODO (ET-208) replace when we have saving to files or uniffi supports Bytes
                ctx.image_for_sender(&sender, size, mode, format)
                    .await
                    .map(|v| v.map(|v| v.to_vec()))
            })
            .await
            .map_err(map_task_join_error)??)
    }
}

fn light_or_dark_mode_from_string(
    mode: Option<String>,
) -> MailSessionResult<Option<LightOrDarkMode>> {
    let mode = match mode {
        Some(m) => match m.as_str() {
            "light" | "Light" => Some(LightOrDarkMode::Light),
            "dark" | "Dark" => Some(LightOrDarkMode::Dark),
            _ => return Err(MailSessionError::InvalidImageMode(m)),
        },
        None => None,
    };
    Ok(mode)
}
