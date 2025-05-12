use crate::cache::{CacheConfig, CacheError, CacheKey, CacheResource, CacheResult};
use crate::datatypes::LightOrDarkMode;
use crate::models::ModelExtension;
use anyhow::anyhow;
use derive_more::derive::TryFrom;
use futures::executor::block_on;
use proton_core_api::services::proton::GetImagesLogoOptions;
use proton_sqlite3::rusqlite::types::{
    FromSql, FromSqlError, FromSqlResult, ToSqlOutput, ValueRef,
};
use stash::exports::{SqliteError, ToSql, Value};
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{Bond, Stash, StashError};
use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use std::vec;
use tracing::{error, warn};

/// Representation of configuration for a sender image
/// Used to persist cache.
/// A record must be present if and only if the corresponding item is in cache.
#[derive(Clone, Debug, Default, Eq, Model)]
#[TableName("sender_image_cache")]
pub struct SenderImage {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    // NOTE: this field is never really used, but we are forced by stash to declare
    // it or the model won't work.
    #[IdField(autoincrement)]
    pub local_id: Option<u64>,

    /// TODO: Document this field.
    #[DbField]
    pub address: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub bimi_selector: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub domain: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub format: Option<String>,

    /// TODO: Document this field.
    #[DbField]
    pub max_scale_up_factor: Option<u8>,

    /// TODO: Document this field.
    #[DbField]
    pub mode: Option<LightOrDarkMode>,

    /// TODO: Document this field.
    #[DbField]
    pub size: Option<u32>,

    /// Format received from backend (png, svg or webp)
    #[DbField]
    pub received_format: Option<ReceivedFormat>,

    /// Received file is empty (happen when backend can't find an item corresponding to the request)
    #[DbField]
    pub is_empty: bool,

    /// The internal row ID of the record in the database. This is assigned by `SQLite`, and is used
    /// as a consistent identifier for records when listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

impl SenderImage {
    /// Remove all given values from Database.
    ///
    /// # Params
    /// * `values` - list of values to remove
    ///
    /// # Errors
    /// * if a database request fail.
    ///
    pub async fn batch_delete(
        values: impl IntoIterator<Item = Self>,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        for value in values {
            value.delete(bond).await?;
        }
        Ok(())
    }

    /// Remove a record from the table.
    ///
    /// # Error
    /// * If the database request fail.
    ///
    pub(crate) async fn delete(&self, bond: &Bond<'_>) -> Result<(), StashError> {
        bond.execute(
            r"DELETE FROM sender_image_cache WHERE local_id = ?",
            params![self.local_id],
        )
        .await?;
        Ok(())
    }

    /// Update the current value of `received_format`
    ///
    /// N.B.: It's necessary since `PartialEq` exclude `received_format` and `is_empty` from
    ///       equality test
    ///
    pub(crate) async fn set_metadata(
        &mut self,
        metadata: &SenderImageMetadata,
        bond: &Bond<'_>,
    ) -> Result<(), StashError> {
        self.received_format = Some(metadata.received_format);
        self.is_empty = metadata.is_empty;
        bond.execute(
            "UPDATE sender_image_cache SET received_format = ?, is_empty = ? WHERE local_id = ?",
            params![self.received_format, self.is_empty, self.local_id],
        )
        .await?;
        Ok(())
    }

    /// Create a query to request a `SenderImage`
    #[must_use]
    pub fn build_query(&self) -> (String, Vec<Box<dyn ToSql + Send>>) {
        fn build_field<T>(field: Option<&T>, name: &str) -> String {
            if field.is_some() {
                format!("{name} = ?")
            } else {
                format!("{name} IS NULL")
            }
        }

        let address = build_field(self.address.as_ref(), "address");
        let bimi = build_field(self.bimi_selector.as_ref(), "bimi_selector");
        let domain = build_field(self.domain.as_ref(), "domain");
        let format = build_field(self.format.as_ref(), "format");
        let max = build_field(self.max_scale_up_factor.as_ref(), "max_scale_up_factor");
        let mode = build_field(self.mode.as_ref(), "mode");
        let size = build_field(self.size.as_ref(), "size");

        let mut params: Vec<Box<dyn ToSql + Send>> = Vec::new();
        if self.address.is_some() {
            params.push(Box::new(self.address.clone()));
        }
        if self.bimi_selector.is_some() {
            params.push(Box::new(self.bimi_selector.clone()));
        }
        if self.domain.is_some() {
            params.push(Box::new(self.domain.clone()));
        }
        if self.format.is_some() {
            params.push(Box::new(self.format.clone()));
        }
        if self.max_scale_up_factor.is_some() {
            params.push(Box::new(self.max_scale_up_factor));
        }
        if self.mode.is_some() {
            params.push(Box::new(self.mode));
        }
        if self.size.is_some() {
            params.push(Box::new(self.size));
        }

        (
            format!(
                "WHERE {address} AND {bimi} AND {domain} AND {format} AND {max} AND {mode} AND {size}",
            ),
            params,
        )
    }

    /// Save or update a `SenderImage`.
    ///
    /// It's imperative that you use this method over [`Model::save()`] to ensure that the
    /// information is update correctly in the database.
    ///
    /// # Errors
    ///
    /// Returns error if a database request fail.
    ///
    #[allow(clippy::missing_panics_doc)]
    pub async fn save(&mut self, bond: &Bond<'_>) -> Result<(), StashError> {
        let (query, params) = self.build_query();
        let mut values = Self::find(query, params, bond).await?;

        match values.len() {
            0 => <Self as Model>::save(self, bond).await?,
            1 => {
                let value = values.get_mut(0).expect("One item present").clone();
                self.local_id = value.local_id;
                self.row_id = value.row_id;
            }
            _ => {
                return Err(StashError::Critical(anyhow!(
                    "Custom Unique constraint for SenderImage failed"
                )));
            }
        }

        Ok(())
    }
}

// Exclude `received_format`, `row_id` and `stash` so when used as a key in ProtonCache, those
// values don't make it different keys.
impl Hash for SenderImage {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.address.hash(state);
        self.bimi_selector.hash(state);
        self.domain.hash(state);
        self.format.hash(state);
        self.max_scale_up_factor.hash(state);
        self.mode.hash(state);
        self.size.hash(state);
    }
}

// Exclude `received_format`, `row_id` and `stash` so when used as a key in ProtonCache, those
// values don't make it different keys.
//
// N.B.: Excluding `received_format` prevent `<Model>::save()` to work when only that value change.
//   So use Self::update_received_format
impl PartialEq for SenderImage {
    fn eq(&self, other: &Self) -> bool {
        self.address == other.address
            && self.bimi_selector == other.bimi_selector
            && self.domain == other.domain
            && self.format == other.format
            && self.max_scale_up_factor == other.max_scale_up_factor
            && self.mode == other.mode
            && self.size == other.size
    }
}

impl From<&GetImagesLogoOptions> for SenderImage {
    fn from(value: &GetImagesLogoOptions) -> Self {
        Self {
            address: value.address.clone(),
            bimi_selector: value.bimi_selector.clone(),
            domain: value.domain.clone(),
            format: value.format.clone(),
            max_scale_up_factor: value.max_scale_up_factor,
            mode: value.mode.map(Into::into),
            size: value.size,
            ..Default::default()
        }
    }
}

impl From<&SenderImage> for GetImagesLogoOptions {
    fn from(value: &SenderImage) -> Self {
        Self {
            address: value.address.clone(),
            bimi_selector: value.bimi_selector.clone(),
            domain: value.domain.clone(),
            format: value.format.clone(),
            max_scale_up_factor: value.max_scale_up_factor,
            mode: value.mode.map(Into::into),
            size: value.size,
        }
    }
}

impl CacheConfig for SenderImage {
    type Key = Self;
    type Resource = Stash;
    type ExtraMetadata = SenderImageMetadata;

    async fn get_existing(stash: Stash) -> CacheResult<Vec<Self::Key>> {
        let conn = stash.connection();
        Self::all(&conn)
            .await
            .map_err(|e| CacheError::Callback(anyhow!(e)))
    }

    async fn handle_failed(failed: Vec<Self::Key>, stash: Stash) -> CacheResult<()> {
        let mut conn = stash.connection();
        let tx = conn.transaction().await?;
        Self::batch_delete(failed, &tx).await?;
        tx.commit().await?;

        Ok(())
    }

    fn key_to_filename(
        key: &Self::Key,
        extra: Option<&Self::ExtraMetadata>,
    ) -> CacheResult<OsString> {
        let id = key
            .local_id
            .ok_or(CacheError::Callback(anyhow!("LocalId is not initialized")))?;
        let extra = extra.ok_or(CacheError::NeedExtraMetadata)?;
        Ok(format!("{id}.{}", extra.received_format).into())
    }

    fn extra_for_key(key: &Self::Key) -> Option<Self::ExtraMetadata> {
        SenderImageMetadata::try_from(key)
            .inspect_err(|()| warn!("Can't get extra metadata for {key:?}"))
            .ok()
    }
}

impl CacheKey for SenderImage {
    fn after_evict<R: CacheResource>(&self, resource: R) {
        block_on(async {
            // TODO: This block on may be trublesome as it may hit on Database is blocked as was the case in event loop
            let mut conn = resource.stash().unwrap().connection();
            let tx = conn.transaction().await.unwrap();
            let _ = self
                .delete(&tx)
                .await
                .inspect_err(|e| error!("Couldn't delete {self:?} from database: {e:?}"));
            tx.commit().await.unwrap();
        });
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, TryFrom)]
#[try_from(repr)]
#[repr(u8)]
pub enum ReceivedFormat {
    Png = 1,
    Svg = 2,
    WebP = 3,
}

impl From<&[u8]> for ReceivedFormat {
    fn from(value: &[u8]) -> Self {
        if value.len() > 7 {
            match value[0..4] {
                // 89 50 4E 47 0D 0A 1A 0A	=> PNG ,
                [0x89, 0x50, 0x4E, 0x47] => {
                    if value[4..8] == [0x0D, 0x0A, 0x1A, 0x0A] {
                        return ReceivedFormat::Png;
                    }
                }
                // 52 49 46 46 ?? ?? ?? ?? 57 45 42 50 => WebP
                [0x52, 0x49, 0x46, 0x46] => {
                    if value.len() > 11 && value[8..12] == [0x57, 0x45, 0x42, 0x50] {
                        return ReceivedFormat::WebP;
                    }
                }
                _ => (),
            }
        }
        ReceivedFormat::Svg
    }
}

impl Display for ReceivedFormat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            ReceivedFormat::Png => write!(f, "png"),
            ReceivedFormat::Svg => write!(f, "svg"),
            ReceivedFormat::WebP => write!(f, "webp"),
        }
    }
}

impl FromSql for ReceivedFormat {
    fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
        let val = u8::column_result(value)?;
        Self::try_from(val).map_err(|_| FromSqlError::OutOfRange(i64::from(val)))
    }
}

impl ToSql for ReceivedFormat {
    fn to_sql(&self) -> Result<ToSqlOutput<'_>, SqliteError> {
        Ok(ToSqlOutput::Owned(Value::Integer(*self as i64)))
    }
}

#[derive(Clone, Debug)]
pub struct SenderImageMetadata {
    pub received_format: ReceivedFormat,
    pub is_empty: bool,
}

impl TryFrom<&SenderImage> for SenderImageMetadata {
    type Error = ();

    fn try_from(value: &SenderImage) -> Result<Self, Self::Error> {
        if let Some(received_format) = value.received_format {
            Ok(Self {
                received_format,
                is_empty: value.is_empty,
            })
        } else {
            Err(())
        }
    }
}
