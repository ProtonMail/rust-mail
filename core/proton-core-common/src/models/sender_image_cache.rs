use crate::cache::{CacheConfig, CacheError, CacheKey, CacheResult};
use crate::datatypes::{LightOrDarkMode, LocalId};
use crate::models::ModelExtension;
use anyhow::anyhow;
use futures::executor::block_on;
use proton_api_core::services::proton::requests::GetImagesLogoOptions;
use stash::exports::ToSql;
use stash::macros::Model;
use stash::orm::Model;
use stash::params;
use stash::stash::{AgnosticInterface, Interface, Stash, StashError};
use std::ffi::OsString;
use std::hash::{Hash, Hasher};
use std::vec;
use tracing::error;

/// Representation of configuration for a sender image
/// Used to persist cache.
/// A record must be present if and only if the corresponding item is in cache.
#[derive(Clone, Debug, Default, Eq, Model, PartialEq)]
#[TableName("sender_image_cache")]
pub struct SenderImage {
    /// The local ID of the record, i.e. the ID assigned by the client
    /// application. This is a restricted-scope unique identifier for the record
    /// within the set of all records of this type, and is important for
    /// relating local records. It has no relationship to the centrally-stored
    /// API ID, and never leaves the local system.
    #[IdField(autoincrement)]
    pub local_id: Option<LocalId>,

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

    /// The internal row ID of the record in the database. This is assigned by `SQLite`, and is used
    /// as a consistent identifier for records when listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,

    /// The database instance that the record is associated with. This is present for convenience.
    #[StashField]
    pub stash: Option<Stash>,
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
    pub async fn batch_delete(values: impl IntoIterator<Item = Self>) -> Result<(), StashError> {
        for value in values {
            value.delete().await?;
        }
        Ok(())
    }

    /// Remove a record from the table.
    ///
    /// # Error
    /// * If the database request fail.
    ///
    pub(crate) async fn delete(&self) -> Result<(), StashError> {
        let interface = self.stash.clone().ok_or(StashError::NoStashAvailable)?;
        interface
            .execute(
                r"DELETE FROM sender_image_cache WHERE local_id = ?",
                params![self.local_id],
            )
            .await?;
        Ok(())
    }

    /// Create a query to request a `SenderImage`
    #[must_use]
    pub fn build_query(&self) -> (String, Vec<Box<dyn ToSql + Send>>) {
        fn build_field<T>(field: &Option<T>, name: &str) -> String {
            if field.is_some() {
                format!("{name} = ?")
            } else {
                format!("{name} IS NULL")
            }
        }

        let address = build_field(&self.address, "address");
        let bimi = build_field(&self.bimi_selector, "bimi_selector");
        let domain = build_field(&self.domain, "domain");
        let format = build_field(&self.format, "format");
        let max = build_field(&self.max_scale_up_factor, "max_scale_up_factor");
        let mode = build_field(&self.mode, "mode");
        let size = build_field(&self.size, "size");

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
    pub async fn save(&mut self) -> Result<(), StashError> {
        let Some(stash) = self.stash.clone() else {
            return Err(StashError::NoStashAvailable);
        };

        self.save_using(&stash).await
    }

    /// Save or update a `SenderImage`.
    ///
    /// It's imperative that you use this method over [`Model::save_using()`] to ensure that the
    /// information is update correctly in the database.
    ///
    /// # Errors
    ///
    /// Returns error if a database request fail.
    ///
    #[allow(clippy::missing_panics_doc)]
    pub async fn save_using<A>(&mut self, interface: &A) -> Result<(), StashError>
    where
        A: Into<AgnosticInterface> + Interface,
    {
        let (query, params) = self.build_query();
        if self.stash.is_none() {
            self.set_stash(interface.stash());
        }

        let transaction = interface.transaction().await?;
        let mut values = Self::find(query, params, &transaction, None).await?;
        match values.len() {
            0 => <Self as Model>::save_using(self, &transaction).await?,
            1 => {
                let value = values.get_mut(0).expect("One item present").clone();
                self.local_id = value.local_id;
                self.stash.clone_from(&value.stash);
                self.row_id = value.row_id;
            }
            _ => {
                return Err(StashError::Custom(
                    "Custom Unique constraint for SenderImage failed".to_owned(),
                ))
            }
        }
        transaction.commit().await?;
        Ok(())
    }
}

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

impl From<SenderImage> for GetImagesLogoOptions {
    fn from(value: SenderImage) -> Self {
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
    type Init = Stash;

    async fn get_existing(stash: Stash) -> CacheResult<Vec<Self::Key>> {
        Self::all(&stash, None)
            .await
            .map_err(|e| CacheError::Callback(anyhow!(e)))
    }

    async fn handle_failed(failed: Vec<Self::Key>) -> CacheResult<()> {
        Self::batch_delete(failed)
            .await
            .map_err(|e| CacheError::Callback(anyhow!(e)))
    }

    fn key_to_filename(key: &Self::Key) -> OsString {
        let id = key.local_id.expect("Should be set");
        format!("{id}").into()
    }
}

impl CacheKey for SenderImage {
    fn after_evict(&self) {
        block_on(async {
            let _ = self
                .delete()
                .await
                .inspect_err(|e| error!("Couldn't delete {self:?} from database: {e}"));
        });
    }
}
