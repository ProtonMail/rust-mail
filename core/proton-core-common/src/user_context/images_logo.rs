use crate::datatypes::LightOrDarkMode;
use crate::models::sender_image_cache::SenderImage;
use crate::{CoreContextResult, UserContext};
use proton_api_core::services::proton::requests::GetImagesLogoOptions;
use proton_api_core::session::CoreSession;
use stash::stash::{AgnosticInterface, Interface};
use std::path::PathBuf;

impl UserContext {
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
    /// * `address`       - Email address of the sender.
    /// * `bimi_selector` - BIMI protocol selector.
    /// * `format`        - Desired image format, if none is specified the default format of the
    ///                     image will be used.
    /// * `mode`          - Can be used to select if the "light" or "dark" mode of the image is
    ///                     desired (default is light).
    /// * `size`          - Is used to give the x*x size of the returned image (will default to 32
    ///                     if none provided).
    /// * `interface`     - The database interface, i.e. [`Stash`] or [`Tether`], to use for finding
    ///                     the records.
    ///
    /// # Errors
    /// Returns errors if the API call fails, the conversation doesn't exist, or if there's an
    /// issue with the sender that causes problems when creating the API request on our side.
    pub async fn image_for_sender<A>(
        &self,
        address: &str,
        bimi_selector: Option<&str>,
        format: Option<String>,
        mode: Option<LightOrDarkMode>,
        size: Option<u32>,
        interface: &A,
    ) -> CoreContextResult<PathBuf>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let options = GetImagesLogoOptions {
            address: Some(address.to_owned()),
            bimi_selector: bimi_selector.map(ToOwned::to_owned),
            format,
            mode: mode.map(Into::into),
            size,
            ..Default::default()
        };

        let mut key: SenderImage = (&options).into();
        key.save_using(interface).await?;
        if let Some(file) = self.images_logo_cache.get_item_path(&key) {
            Ok(file)
        } else {
            let user_image = self
                .session()
                .api()
                .get_images_logo(options.clone())
                .await?;
            Ok(self.images_logo_cache.add_item(key, user_image.as_ref())?)
        }
    }
}
