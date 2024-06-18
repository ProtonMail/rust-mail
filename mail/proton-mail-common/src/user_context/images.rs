use crate::{MailContextResult, MailUserContext};
use bytes::Bytes;
use proton_api_mail::domain::{AddressDomainLogoDetailsBuilder, LightOrDarkMode, MailSettings};

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
    ///
    /// # Errors
    /// Returns errors if the API call fails, the conversation doesn't exist, or if there's an
    /// issue with the sender that causes problems when creating the API request on our side.
    #[allow(clippy::too_many_arguments)]
    pub async fn image_for_sender(
        &self,
        mail_settings: &MailSettings,
        address: String,
        bimi_selector: Option<String>,
        display_sender_image: bool,
        size: Option<u32>,
        mode: Option<LightOrDarkMode>,
        format: Option<String>,
    ) -> MailContextResult<Option<Bytes>> {
        if mail_settings.hide_sender_images {
            // sender images are to be hidden, return nothing
            return Ok(None);
        }

        if !display_sender_image {
            return Ok(None);
        }

        let mut address_request_details = AddressDomainLogoDetailsBuilder::new().address(address);

        if let Some(s) = size {
            address_request_details = address_request_details.size(s);
        }

        if let Some(m) = mode {
            address_request_details = address_request_details.mode(m);
        }

        if let Some(bimi_sel) = bimi_selector {
            address_request_details = address_request_details.bimi_selector(bimi_sel);
        }

        if let Some(format) = format {
            address_request_details = address_request_details.format(format)
        }

        let address_request_details = address_request_details.build()?;

        Ok(Some(
            self.mail_session()
                .get_address_domain_logo(address_request_details)
                .await?,
        ))
    }
}
