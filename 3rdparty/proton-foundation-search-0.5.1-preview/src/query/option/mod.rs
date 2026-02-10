//! Query options for the search engine

pub mod text;
#[cfg(feature = "wasm-bindgen")]
mod wasm;

use std::any::{Any, type_name};
use std::collections::BTreeMap;
use std::fmt::Debug;

/// Extensible set of query options.
///
/// TODO: Type safety is great, but let's add wasm friendly methods as well.
#[derive(Debug, Default)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct QueryOptions {
    options: BTreeMap<Box<str>, Box<dyn QueryOption>>,
}

/// Marks query options
pub trait QueryOption: Any + Debug + Send {}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl QueryOptions {
    /// Create new default query options
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(constructor)
    )]
    pub fn new() -> Self {
        Self::default()
    }
}

impl QueryOptions {
    /// Modify query options
    ///
    /// Example:
    ///
    /// ```rust
    /// use proton_foundation_search::query::option::{QueryOption, QueryOptions};
    /// #[derive(Debug, Default)]
    /// struct FeatureX {
    ///     enabled: bool,
    /// }
    /// impl QueryOption for FeatureX {}
    /// #[derive(Debug, Default)]
    /// struct FeatureY {
    ///     token: String,
    /// }
    /// impl QueryOption for FeatureY {}
    ///
    /// let opts = QueryOptions::default()
    ///     .with::<FeatureX>(|x| x.enabled = true)
    ///     .with::<FeatureY>(|y| y.token = "ham".into());
    /// ```
    pub fn with<O>(mut self, update: impl FnOnce(&mut O)) -> Self
    where
        O: Default + QueryOption,
    {
        let option = self.get_mut();
        (update)(option);
        self
    }

    /// Get a configured option
    /// Example:
    ///
    /// ```rust
    /// use proton_foundation_search::query::option::{QueryOption, QueryOptions};
    /// #[derive(Debug, Default)]
    /// struct FeatureX;
    /// impl QueryOption for FeatureX {}
    ///
    /// let opts = QueryOptions::default().with::<FeatureX>(|_| ());
    ///
    /// let x = opts.get::<FeatureX>().unwrap();
    /// ```
    pub fn get<O>(&self) -> Option<&O>
    where
        O: QueryOption,
    {
        let name = type_name::<O>();
        self.options.get(name).map(|opt| {
            (opt.as_ref() as &dyn Any)
                .downcast_ref()
                .unwrap_or_else(|| panic!("option get {name} downcast"))
        })
    }

    /// Get a configured option with a mutable reference
    /// Example:
    ///
    /// ```rust
    /// use proton_foundation_search::query::option::{QueryOption, QueryOptions};
    /// #[derive(Debug, Default)]
    /// struct FeatureX {
    ///     enabled: bool,
    /// }
    /// impl QueryOption for FeatureX {}
    ///
    /// let mut opts = QueryOptions::default().with::<FeatureX>(|_| ());
    ///
    /// let x = opts.get_mut::<FeatureX>();
    ///
    /// x.enabled = true;
    /// ```
    pub fn get_mut<O>(&mut self) -> &mut O
    where
        O: Default + QueryOption,
    {
        let name: Box<str> = type_name::<O>().into();
        let option = self
            .options
            .entry(name.clone())
            .or_insert_with(|| Box::new(O::default()))
            .as_mut() as &mut dyn Any;
        option
            .downcast_mut()
            .unwrap_or_else(|| panic!("option get_mut {name} downcast"))
    }

    /// set given option
    pub fn set<O: QueryOption>(&mut self, option: O) {
        let name = type_name::<O>().into();
        self.options.insert(name, Box::new(option));
    }

    /// add/replace provided options
    pub fn extend(&mut self, other: QueryOptions) {
        self.options.extend(other.options);
    }
}
