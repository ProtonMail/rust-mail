use crate::datatypes::LightOrDarkMode;
use crate::os::safe_write_async;
use crate::{CoreContextResult, UserContext};
use anyhow::Context as _;
use indoc::indoc;
use proton_core_api::services::proton::prelude::GetImagesLogoOptions;
use proton_core_api::services::proton::{PrivateEmail, ProtonCore};
use stash::exports::{SqliteError, ToSql};
use stash::params;
use stash::stash::{Bond, StashError, Tether};
use std::hash::{DefaultHasher, Hash, Hasher};
use tracing::info;

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
    ///
    pub async fn image_for_sender(
        &self,
        address: PrivateEmail,
        bimi_selector: Option<String>,
        format: Option<String>,
        mode: Option<LightOrDarkMode>,
        size: Option<u32>,
        tether: &mut Tether,
    ) -> CoreContextResult<Option<String>> {
        // FIXME: ET-2209:
        // Cache for sender images older than 1 day must be invalidated, but only when online.

        match find_sender_image_in_cache(
            tether,
            address.clone(),
            bimi_selector.clone(),
            format.clone(),
            mode,
            size,
        )
        .await
        .with_context(|| format!("Error in finding sender image for {address}"))?
        {
            ImageInCache::NotCached => (),
            ImageInCache::EmptyImage => return Ok(None),
            ImageInCache::Cached { path } => return Ok(Some(path)),
        }
        info!("Not cached, requesting...");

        // It was not cached, let's ask the API for it
        let options = GetImagesLogoOptions {
            address: Some(address.clone()),
            bimi_selector: bimi_selector.clone(),
            format: format.clone(),
            mode: mode.map(Into::into),
            size,
            ..Default::default()
        };

        // If the request fails we don't store anything into the database, let the clients handle
        // it.
        let image = self.session().get_images_logo(options).await?;
        tether
            .tx(async |tx| {
                match find_sender_image_in_cache(
                    tx,
                    address.clone(),
                    bimi_selector.clone(),
                    format.clone(),
                    mode,
                    size,
                )
                .await
                .with_context(|| format!("Error in finding sender image for {address}"))?
                {
                    ImageInCache::NotCached => (),
                    // If you're wondering about the transaction, it's fine to rollback since
                    // we only use it as a lock mechanism.
                    ImageInCache::EmptyImage => return Ok(None),
                    ImageInCache::Cached { path } => return Ok(Some(path)),
                }

                // We don't insert the path as the image doesn't have one.
                if image.is_empty() {
                    insert(tx, address, bimi_selector, mode, size, format, None).await?;
                    return Ok(None);
                }

                let image_format = format_from_bytes(&image);
                // The path exists since it's created in UserContext::new

                // Get the hash of the sender image.
                // The hash is also the primary id of the sender image table.
                let mut state = DefaultHasher::new();
                bimi_selector.hash(&mut state);
                image_format.hash(&mut state);
                mode.hash(&mut state);
                size.hash(&mut state);
                #[allow(clippy::cast_possible_wrap)]
                let opts_hash = state.finish() as i64;

                // We will write the image to
                // {CACHE_PATH}/sender_images/{hash in hex}.{svg/webp/png}
                let path = self.sender_images_cache_path().join(format!(
                    "{}-{opts_hash:#x}.{image_format}",
                    address.as_clear_text_str()
                ));

                let image_size = image.len();
                safe_write_async(&path, image).await.with_context(|| {
                    format!("Error saving {image_size} bytes to {}", path.display())
                })?;
                let path = path
                    .into_os_string()
                    .into_string()
                    // This is infailable since all pieces exist as a string at some point.
                    .map_err(|e| anyhow::anyhow!("Invalid utf8 somewhere in the path: {e:?}"))?;

                insert(
                    tx,
                    address,
                    bimi_selector,
                    mode,
                    size,
                    format,
                    Some(path.clone()),
                )
                .await?;

                Ok(Some(path))
            })
            .await
    }
}

async fn insert(
    tx: &Bond<'_>,
    address: PrivateEmail,
    bimi_selector: Option<String>,
    mode: Option<LightOrDarkMode>,
    size: Option<u32>,
    format: Option<String>,
    path: Option<String>,
) -> CoreContextResult<()> {
    tx.execute(
        indoc! {
        "INSERT INTO sender_image_cache (address, bimi_selector, format, mode, size, path)
                VALUES (?, ?, ?, ?, ?, ?)",
                },
        params![address, bimi_selector, format, mode, size, path],
    )
    .await
    .context("Error inserting sender image")?;
    Ok(())
}

enum ImageInCache {
    NotCached,
    EmptyImage,
    Cached { path: String },
}

async fn find_sender_image_in_cache(
    tether: &Tether,
    address: PrivateEmail,
    bimi_selector: Option<String>,
    format: Option<String>,
    mode: Option<LightOrDarkMode>,
    size: Option<u32>,
) -> Result<ImageInCache, StashError> {
    let sender_image = select(tether, address, bimi_selector, format, mode, size).await;

    match sender_image {
        Err(StashError::ExecutionError(SqliteError::QueryReturnedNoRows)) => {
            Ok(ImageInCache::NotCached)
        }
        Ok(Some(path)) => Ok(ImageInCache::Cached { path }),
        Ok(None) => Ok(ImageInCache::EmptyImage),
        Err(e) => Err(e),
    }
}

async fn select(
    tether: &Tether,
    address: PrivateEmail,
    bimi_selector: Option<String>,
    format: Option<String>,
    mode: Option<LightOrDarkMode>,
    size: Option<u32>,
) -> Result<Option<String>, StashError> {
    let mut params: Vec<Box<dyn ToSql + Send>> = vec![Box::new(address)];
    let mut query_parts = vec![indoc! {
    "SELECT path FROM sender_image_cache
                 WHERE address = ?"}];

    macro_rules! push_option {
        ($arg: ident) => {
            if let Some(val) = $arg {
                params.push(Box::new(val));
                query_parts.push(concat!(stringify!($arg), " = ?"));
            } else {
                query_parts.push(concat!(stringify!($arg), " IS NULL"));
            }
        };
    }

    push_option!(bimi_selector);
    push_option!(format);
    push_option!(mode);
    push_option!(size);

    let query: String = query_parts.join(" AND ");

    tether.query_value(query, params).await
}

fn format_from_bytes(bytes: &[u8]) -> &'static str {
    if bytes.len() > 7 {
        match bytes[0..4] {
            // 89 50 4E 47 0D 0A 1A 0A	=> PNG ,
            [0x89, 0x50, 0x4E, 0x47] => {
                if bytes[4..8] == [0x0D, 0x0A, 0x1A, 0x0A] {
                    return "png";
                }
            }
            // 52 49 46 46 ?? ?? ?? ?? 57 45 42 50 => WebP
            [0x52, 0x49, 0x46, 0x46] => {
                if bytes.len() > 11 && bytes[8..12] == [0x57, 0x45, 0x42, 0x50] {
                    return "webp";
                }
            }
            _ => (),
        }
    }
    "svg"
}
