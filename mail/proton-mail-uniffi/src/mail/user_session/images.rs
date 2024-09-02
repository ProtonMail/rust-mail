use crate::mail::datatypes::MailSettings;
use crate::mail::MailUserSession;
use crate::mail::{MailSessionError, MailSessionResult};
use crate::uniffi_async;
use anyhow::anyhow;
use proton_core_common::datatypes::LightOrDarkMode;

#[uniffi::export]
impl MailUserSession {
    /// Get a path to the sender image.
    ///
    /// # Parameters
    /// * `address`: Email address of the sender.
    /// * `bimi_selector`: BIMI protocol selector.
    /// * `display_sender_image`: Whether this sender would has sender image enabled.
    /// * `size`: Is used to give the x*x size of the returned image (will default to 32 if none provided).
    /// * `mode`: Can be used to select if the "light" or "dark" mode of the image is desired (default is light).
    /// * `format`: Desired image format, if none is specified the default format of the image will be used.
    ///
    /// Returns a path toward the image file or `None` if no image needs to be displayed.
    ///
    /// # Errors
    /// Returns errors if the API call fails, the mode value is invalid, the conversation doesn't exist, or
    /// if there's an issue with the sender that causes problems when creating the API request on our side.
    /// Also returns errors if the path can't be converted into a string.
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
    ) -> MailSessionResult<Option<String>> {
        let ctx = self.ctx.clone();
        uniffi_async(async move {
            let mode = light_or_dark_mode_from_string(mode)?;
            if let Some(path) = ctx
                .image_for_sender(
                    &mail_settings.clone().into(),
                    address,
                    bimi_selector.as_deref(),
                    display_sender_image,
                    size,
                    mode,
                    format,
                )
                .await?
            {
                Ok(Some(
                    path.to_str()
                        .ok_or(MailSessionError::Other(anyhow!(
                            "Can't convert image path into string"
                        )))?
                        .to_owned(),
                ))
            } else {
                Ok(None)
            }
        })
        .await
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
