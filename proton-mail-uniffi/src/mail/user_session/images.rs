use crate::mail::datatypes::MailSettings;
use crate::mail::MailUserSession;
use crate::mail::{MailSessionError, MailSessionResult};
use proton_core_common::datatypes::LightOrDarkMode;
use std::os::unix::ffi::OsStrExt;

#[uniffi::export]
impl MailUserSession {
    /// Get the sender image for a list of senders.
    ///
    /// # Parameters
    /// * `address`: Email address of the sender.
    /// * `bimi_selector`: BIMI protocol selector.
    /// * `display_sender_image`: Whether this sender would has sender image enabled.
    /// * `size`: Is used to give the x*x size of the returned image (will default to 32 if none provided).
    /// * `mode`: Can be used to select if the "light" or "dark" mode of the image is desired (default is light).
    /// * `format`: Desired image format, if none is specified the default format of the image will be used.
    ///
    /// Returns `None` if no image needs to be displayed.
    ///
    /// # Errors
    /// Returns errors if the API call fails, the mode value is invalid, the conversation doesn't exist, or
    /// if there's an issue with the sender that causes problems when creating the API request on our side.
    #[allow(clippy::too_many_arguments)]
    pub async fn image_for_sender(
        &self,
        mail_settings: MailSettings,
        address: String,
        bimi_selector: Option<String>,
        display_sender_image: bool,
        size: Option<u32>,
        mode: Option<String>,
        format: Option<String>,
    ) -> MailSessionResult<Option<Vec<u8>>> {
        let mode = light_or_dark_mode_from_string(mode)?;

        //TODO (ET-208) replace when we have saving to files or uniffi supports Bytes
        Ok(self
            .ctx
            .image_for_sender(
                &mail_settings.clone().into(),
                address,
                bimi_selector.as_deref(),
                display_sender_image,
                size,
                mode,
                format,
            )
            .await
            .map(|v| v.map(|v| v.as_os_str().as_bytes().to_vec()))?)
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
