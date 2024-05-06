use crate::{MailContextResult, MailUserContext};
use bytes::Bytes;
use proton_api_mail::domain::{AddressDomainLogoDetailsBuilder, LightOrDarkMode, MessageAddress};

impl MailUserContext {
    /// Get sender image for a list of addresses.
    ///
    /// See [`image_for_address`] for more details.
    ///
    /// # Errors
    /// Returns errors if the API call fails, the conversation doesn't exist, or if there's an
    /// issue with the sender that causes problems when creating the API request on our side.

    pub async fn image_for_senders(
        &self,
        senders: &[MessageAddress],
        size: Option<u32>,
        mode: Option<LightOrDarkMode>,
        format: Option<String>,
    ) -> MailContextResult<Option<Bytes>> {
        let Some(first) = senders.first() else {
            return Ok(None);
        };

        self.image_for_sender(first, size, mode, format).await
    }

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
    /// # Errors
    /// Returns errors if the API call fails, the conversation doesn't exist, or if there's an
    /// issue with the sender that causes problems when creating the API request on our side.
    pub async fn image_for_sender(
        &self,
        sender: &MessageAddress,
        size: Option<u32>,
        mode: Option<LightOrDarkMode>,
        format: Option<String>,
    ) -> MailContextResult<Option<Bytes>> {
        if self.with_mail_settings(|s| s.hide_sender_images) {
            // sender images are to be hidden, return nothing
            return Ok(None);
        }

        if !sender.display_sender_image {
            return Ok(None);
        }

        let mut address_request_details =
            AddressDomainLogoDetailsBuilder::new().address(sender.address.clone());

        if let Some(s) = size {
            address_request_details = address_request_details.size(s);
        }

        if let Some(m) = mode {
            address_request_details = address_request_details.mode(m);
        }

        if let Some(bimi_sel) = &sender.bimi_selector {
            address_request_details = address_request_details.bimi_selector(bimi_sel.clone());
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
