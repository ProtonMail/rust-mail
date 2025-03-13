use crate::cache::{CacheError, CacheResult};
use crate::datatypes::LightOrDarkMode;
use crate::models::sender_image_cache::{ReceivedFormat, SenderImage, SenderImageMetadata};
use crate::{CoreContextResult, UserContext};
use anyhow::anyhow;
use proton_api_core::services::proton::ProtonCore;
use proton_api_core::session::CoreSession;
use stash::stash::{Bond, Stash};
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
    ///   image will be used.
    /// * `mode`          - Can be used to select if the "light" or "dark" mode of the image is
    ///   desired (default is light).
    /// * `size`          - Is used to give the x*x size of the returned image (will default to 32
    ///   if none provided).
    /// * `interface`     - The database interface, i.e. [`Stash`] or [`Tether`], to use for finding
    ///   the records.
    ///
    /// # Errors
    /// Returns errors if the API call fails, the conversation doesn't exist, or if there's an
    /// issue with the sender that causes problems when creating the API request on our side.
    ///
    /// # Panics
    /// If cache metadata are unset
    pub async fn image_for_sender(
        &self,
        address: &str,
        bimi_selector: Option<&str>,
        format: Option<String>,
        mode: Option<LightOrDarkMode>,
        size: Option<u32>,
        stash: &Stash,
    ) -> CoreContextResult<Option<PathBuf>> {
        let mut key = SenderImage {
            address: Some(address.to_owned()),
            bimi_selector: bimi_selector.map(ToOwned::to_owned),
            format,
            mode,
            size,
            ..Default::default()
        };

        // generate local_id if not exist
        let mut conn = stash.connection();
        let tx = conn.transaction().await?;
        if key.local_id.is_none() {
            key.save(&tx).await?;
        }

        let result = self
            .images_logo_cache
            .get_path_or_insert_with_extra(&key, self.get_images_logo(key.clone(), &tx))
            .await?;
        tx.commit().await?;

        let metadata = self
            .images_logo_cache
            .get_extra_metadata(&key)
            .expect("Should be set");

        if metadata.is_empty {
            Ok(None)
        } else {
            Ok(Some(result))
        }
    }

    /// Get the logo corresponding to the given key
    async fn get_images_logo(
        &self,
        mut key: SenderImage,
        bond: &Bond<'_>,
    ) -> CacheResult<(Vec<u8>, SenderImageMetadata)> {
        let image = self
            .session()
            .api()
            .get_images_logo((&key).into())
            .await
            .map(|v| v.to_vec())
            .map_err(|e| CacheError::Callback(anyhow!(e)))?;
        let received_format = ReceivedFormat::from(image.as_slice());
        let metadata = SenderImageMetadata {
            received_format,
            is_empty: image.is_empty(),
        };
        key.set_metadata(&metadata, bond)
            .await
            .map_err(|e| CacheError::Callback(anyhow!(e)))?;
        Ok((image, metadata))
    }
}
