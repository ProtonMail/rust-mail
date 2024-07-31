use crate::cache::CacheConfig;
use crate::datatypes::LightOrDarkMode;
use crate::{CoreContextResult, UserContext};
use bytes::Bytes;
use proton_api_core::services::proton::requests::GetImagesLogoOptions;
use proton_api_core::session::CoreSession;
use std::ffi::OsString;
use std::io::Read;
use uuid::Uuid;

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
    /// * `address`: Email address of the sender.
    /// * `bimi_selector`: BIMI protocol selector.
    /// * `size`: Is used to give the x*x size of the returned image (will default to 32 if none provided).
    /// * `mode`: Can be used to select if the "light" or "dark" mode of the image is desired (default is light).
    /// * `format`: Desired image format, if none is specified the default format of the image will be used.
    ///
    /// # Errors
    /// Returns errors if the API call fails, the conversation doesn't exist, or if there's an
    /// issue with the sender that causes problems when creating the API request on our side.
    pub async fn image_for_sender(
        &self,
        address: &str,
        bimi_selector: Option<&str>,
        format: Option<String>,
        mode: Option<LightOrDarkMode>,
        size: Option<u32>,
    ) -> CoreContextResult<Option<Bytes>> {
        let options = GetImagesLogoOptions {
            address: Some(address.to_owned()),
            bimi_selector: bimi_selector.map(ToOwned::to_owned),
            format,
            mode: mode.map(Into::into),
            size,
            ..Default::default()
        };

        if let Some(mut file) = self.images_logo_cache.get_item(&options)? {
            let mut user_image = Vec::new();
            file.read_to_end(&mut user_image)?;
            Ok(Some(user_image.into()))
        } else {
            let user_image = self
                .session()
                .api()
                .get_images_logo(options.clone())
                .await?;
            self.images_logo_cache
                .add_item(options, user_image.as_ref())?;
            Ok(Some(user_image))
        }
    }
}

/// Cache key for User Images
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct Key(pub GetImagesLogoOptions);
impl CacheConfig for Key {
    type Key = GetImagesLogoOptions;
    type ExtraMetadata = ();

    fn key_to_filename(_key: &Self::Key) -> OsString {
        // `AddressDomainLogoDetails` contains to many possible configuration to build a unique filename from it
        Uuid::new_v4().to_string().into()
    }
}
