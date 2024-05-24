#![allow(async_fn_in_trait)]

//! ORM utilities for working with database records.
//!
//! This module provides a set of traits and structs for working with database
//! records. It is used to define the interface for database records, and to
//! provide methods for loading and saving records from the database.
//!
//! Nothing prevents the database-handling interface from being used directly,
//! but it is lower-level. The ORM layer sits on top of the database-handling
//! layer and provides a more convenient and idiomatic interface for working
//! with types that are saved to the database.
//!

use crate::stash::{Stash, StashError, Tether};
use core::any::Any;
use core::fmt::Debug;
use core::iter::repeat;
use indoc::formatdoc;
use rusqlite::ToSql;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::collections::HashMap;
use std::vec::IntoIter;
use uuid::Uuid;

/// A trait for database records.
///
/// This trait is used to define the interface for database records. It provides
/// methods for loading and saving records from the database.
///
/// # Design
///
/// The intention is that the various data fields for the struct should be
/// mapped to the database fields using serde, which will be used for
/// serialisation and deserialisation. A common pattern is for those fields to
/// be public on the struct, but the actual method of management is up to the
/// implementor. Meanwhile, the associated [`Stash`] would usually be stored in
/// a private `stash` field — but this again is up to the implementor.
///
pub trait DbRecord:
    Clone + Debug + DeserializeOwned + PartialEq + Send + Serialize + Sized + Sync
where
    Self: 'static,
{
    /// Gets a list of fields with names and associated values for the record.
    fn fields(&self) -> HashMap<&'static str, Box<dyn ToSql + Send>>;

    /// Gets a list of field names for the record type.
    fn field_names() -> Vec<&'static str>;

    /// Gets a list of field values for the record.
    fn field_values(&self) -> Vec<Box<dyn ToSql + Send>>;

    /// Gets the record's unique ID.
    fn id(&self) -> Uuid;

    /// Gets the name of the ID field for the record type.
    fn id_field_name() -> &'static str;

    /// Loads a record from the database by ID.
    ///
    /// This function retrieves a single record from the database by its unique
    /// ID. It is a convenience method for calling [`Stash::query()`] and then
    /// converting the first result to the desired type `T`.
    ///
    /// If no results are found, [`None`] will be returned. Having no results is
    /// not considered to be an error case.
    ///
    /// After loading, the [`Stash`] will be set against the record instance, so
    /// that instance-based operations have the correct context.
    ///
    /// # Parameters
    ///
    /// * `id`    - The ID of the record to load.
    /// * `stash` - The database, i.e. [`Stash`], to use for loading the record.
    ///             It is necessary to provide this in order to know where to
    ///             load the record from.
    ///
    /// # Errors
    ///
    /// See [`Stash::query()`] for a list of possible errors that can occur when
    /// using this function.
    ///
    /// # See also
    ///
    /// * [`DbRecord::load_using()`]
    ///
    #[must_use]
    async fn load(id: Uuid, stash: &Stash) -> Result<Option<Self>, StashError> {
        let query = formatdoc!(
            "
            SELECT
                *
            FROM
                {}
            WHERE
                {} = ?
            LIMIT
                1
        ",
            Self::table_name(),
            Self::id_field_name(),
        );
        #[allow(trivial_casts)]
        Ok(stash
            .query::<_, Self>(&query, vec![Box::new(id) as Box<dyn ToSql + Send>])
            .await?
            .into_iter()
            .next()
            .map(|mut record| {
                record.set_stash(stash);
                record
            }))
    }

    /// Loads a record from the database by ID, using a specific connection.
    ///
    /// This function retrieves a single record from the database by its unique
    /// ID, using a specific [`Tether`], i.e. connection. It is functionally
    /// equivalent to [`load()`](DbRecord::load()), but allows the query to be
    /// run against an existing connection rather than using a new one.
    ///
    /// For full usage details, see [`load()`](DbRecord::load()).
    ///
    /// Note that, unlike [`load()`](DbRecord::load()), this function does not
    /// set the [`Stash`] on the record instance.
    ///
    /// Note also that the [`Tether`] used will not be stored.
    ///
    /// # Parameters
    ///
    /// * `id`     - The ID of the record to load.
    /// * `tether` - The database connection, i.e. [`Tether`], to use for
    ///              loading the record. This allows an existing connection to
    ///              be used, rather than creating a new one.
    ///
    /// # Errors
    ///
    /// See [`DbRecord::load()`].
    ///
    /// # See also
    ///
    /// * [`DbRecord::load()`]
    ///
    #[must_use]
    async fn load_using(id: Uuid, tether: &Tether) -> Result<Option<Self>, StashError> {
        let query = formatdoc!(
            "
            SELECT
                *
            FROM
                {}
            WHERE
                {} = ?
            LIMIT
                1
        ",
            Self::id_field_name(),
            Self::table_name()
        );
        #[allow(trivial_casts)]
        Ok(tether
            .query::<_, Self>(&query, vec![Box::new(id) as Box<dyn ToSql + Send>])
            .await?
            .into_iter()
            .next())
    }

    /// Saves a record to the database.
    ///
    /// This function saves a single record to the database by its unique ID. It
    /// is a convenience method for calling [`Stash::execute()`] and passing in
    /// the data.
    ///
    /// There are two prerequisites for calling this function:
    ///
    ///   1. The record must have a unique ID. This needs to have been set on
    ///      the record instance, or an error will occur.
    ///   2. The [`Stash`] must be set on the record instance. This is necessary
    ///      to know where to save the record to.
    ///
    /// # Errors
    ///
    /// See [`Stash::query()`] for a list of possible errors that can occur when
    /// using this function.
    ///
    /// # See also
    ///
    /// * [`DbRecord::save_using()`]
    ///
    async fn save(&self) -> Result<(), StashError> {
        let fields = Self::field_names();
        let placeholders = repeat("?")
            .take(fields.len())
            .collect::<Vec<_>>()
            .join(", ");
        let update_fields = fields
            .iter()
            .map(|field| format!("{field} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let query = formatdoc!(
            "
            INSERT INTO
                {} ({})
            VALUES
                ({})
            ON CONFLICT ({}) DO UPDATE SET {}
        ",
            Self::table_name(),
            fields.join(", "),
            placeholders,
            Self::id_field_name(),
            update_fields,
        );
        let _: usize = self
            .stash()
            .execute(
                &query,
                Self::field_values(self)
                    .into_iter()
                    .chain(Self::field_values(self))
                    .collect(),
            )
            .await?;
        Ok(())
    }

    /// Saves a record to the database, using a specific connection.
    ///
    /// This function saves a single record to the database by its unique ID,
    /// using a specific [`Tether`], i.e. connection. It is functionally
    /// equivalent to [`save()`](DbRecord::save()), but allows the query to be
    /// run against an existing connection rather than using a new one.
    ///
    /// For full usage details, see [`save()`](DbRecord::save()).
    ///
    /// Note that the [`Tether`] used will not be stored.
    ///
    /// # Parameters
    ///
    /// * `tether` - The database connection, i.e. [`Tether`], to use for
    ///              loading the record. This allows an existing connection to
    ///              be used, rather than creating a new one.
    ///
    /// # Errors
    ///
    /// See [`DbRecord::save()`].
    ///
    /// # See also
    ///
    /// * [`DbRecord::save()`]
    ///
    async fn save_using(&self, tether: &Tether) -> Result<(), StashError> {
        let fields = Self::field_names();
        let placeholders = repeat("?")
            .take(fields.len())
            .collect::<Vec<_>>()
            .join(", ");
        let update_fields = fields
            .iter()
            .map(|field| format!("{field} = ?"))
            .collect::<Vec<_>>()
            .join(", ");
        let query = formatdoc!(
            "
            INSERT INTO
                {} ({})
            VALUES
                ({})
            ON CONFLICT ({}) DO UPDATE SET {}
        ",
            Self::table_name(),
            fields.join(", "),
            placeholders,
            Self::id_field_name(),
            update_fields
        );
        let _: usize = tether
            .execute(
                &query,
                Self::field_values(self)
                    .into_iter()
                    .chain(Self::field_values(self))
                    .collect(),
            )
            .await?;
        Ok(())
    }

    /// Gets a reference to the database-handling [`Stash`] for the record.
    fn stash(&self) -> &Stash;

    /// Sets the database-handling [`Stash`] for the record.
    ///
    /// # Parameters
    ///
    /// * `stash` - The [`Stash`] to set for the record.
    ///
    fn set_stash(&mut self, stash: &Stash);

    /// Gets the name of the table for the record type.
    fn table_name() -> &'static str;
}

/// A collection of database records.
///
/// This struct is used to represent a collection of [`DbRecord`]s returned from
/// a query — the converted query results, i.e. the [`Rows`](rusqlite::Rows)
/// that have been converted into the desired type `T` — but boxed as [`Any`] so
/// that they can be returned via the oneshot channel. These are downcast
/// immediately at the other end of the channel.
///
/// For more information on how this works, see the documentation for
/// `stash::converter()` (note: this is not a public function).
///
#[derive(Debug)]
pub struct DbRecords(Vec<Box<dyn Any + Send + 'static>>);

impl<T: 'static + Send> FromIterator<Box<T>> for DbRecords {
    fn from_iter<I: IntoIterator<Item = Box<T>>>(iter: I) -> Self {
        Self(
            iter.into_iter()
                .map(|item| {
                    #[allow(clippy::shadow_same)]
                    let item: Box<dyn Any + Send> = item;
                    item
                })
                .collect(),
        )
    }
}

impl IntoIterator for DbRecords {
    type Item = Box<dyn Any + Send + 'static>;
    type IntoIter = IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}
