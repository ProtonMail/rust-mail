use crate::models::MailSettings;
use crate::{MailContextResult, MailUserContext};
use proton_core_api::services::proton::PrivateEmail;
use proton_core_common::datatypes::LightOrDarkMode;

impl MailUserContext {
    /// Get sender image for an address.
    ///
    /// The API request is only made in the case where neither the mail settings nor the particular
    /// sender are configured to prevent a sender image being shown.
    ///
    /// If a logo is to be sought via the API, the logo will be for the first sender in the list
    /// included in the conversation.
    ///
    /// When no logo is available `None` is returned.
    ///
    /// # Params
    /// * `address`: Email address of the sender.
    /// * `bimi_selector`: BIMI protocol selector.
    /// * `display_sender_image`: Whether this sender would has sender image enabled.
    /// * `size`: Is used to give the x*x size of the returned image (will default to 32 if none provided).
    /// * `mode`: Can be used to select if the "light" or "dark" mode of the image is desired (default is light).
    /// * `format`: Desired image format, if none is specified the default format of the image will be used.
    #[allow(clippy::too_many_arguments)]
    pub async fn image_for_sender(
        &self,
        address: PrivateEmail,
        bimi_selector: Option<String>,
        display_sender_image: bool,
        size: Option<u32>,
        mode: Option<LightOrDarkMode>,
        format: Option<String>,
    ) -> MailContextResult<Option<String>> {
        if !display_sender_image {
            return Ok(None);
        }

        let tether = &mut self.user_stash().connection().await?;
        let mail_settings = MailSettings::get_or_default(tether).await;

        if mail_settings.hide_sender_images {
            // sender images are to be hidden, return nothing
            return Ok(None);
        }

        Ok(self
            .user_context
            .image_for_sender(address, bimi_selector, format, mode, size, tether)
            .await?)
    }
}
