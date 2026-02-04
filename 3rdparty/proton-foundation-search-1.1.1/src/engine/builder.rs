//! Building an engine with a builder pattern.
//!
//! This module includes the build stages and their implementations.

use std::collections::btree_map::Entry;

use super::*;

/// Builder for the engine.
///
/// # Example
///
/// ```rust
/// use proton_foundation_search::engine::*;
///
/// let _engine = Engine::builder().build();
/// ```
#[derive(Debug)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct EngineBuilder {}

/// An engine builder with schema and processor set
#[derive(Debug)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct EngineBuilderWithProc {
    processor: Box<dyn Processor>,
}

/// An engine builder with schema, processor and indices set
#[derive(Debug)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct EngineBuilderWithIndices {
    processor: Box<dyn Processor>,
    indices: BTreeMap<Box<str>, Arc<dyn Index>>,
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl EngineBuilder {
    /// Configure engine's processor
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withBuiltinProcessor")
    )]
    pub fn with_builtin_processor(self, config: &ProcessorConfig) -> EngineBuilderWithProc {
        EngineBuilderWithProc {
            processor: Box::new(TextProcessor::new(config)),
        }
    }
    /// Shortcut to finish the build with defaults and produce an engine
    pub fn build(self) -> Engine {
        self.with_builtin_processor(&Default::default())
            .with_default_indices()
            .build()
    }
}

impl EngineBuilder {
    /// Configure engine's processor
    pub fn with_processor(self, processor: impl 'static + Processor) -> EngineBuilderWithProc {
        EngineBuilderWithProc {
            processor: Box::new(processor),
        }
    }
}

impl EngineBuilderWithProc {
    /// Add engine's index
    pub fn with_index(self, index: impl 'static + Index) -> EngineBuilderWithIndices {
        let Self { processor } = self;
        EngineBuilderWithIndices {
            processor,
            indices: [(index.id().into(), Arc::new(index) as Arc<dyn Index>)]
                .into_iter()
                .collect(),
        }
    }
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl EngineBuilderWithProc {
    /// Add the default set of indices to the engine's
    pub fn with_default_indices(self) -> EngineBuilderWithIndices {
        let Self { processor } = self;
        let defaults: [Arc<dyn Index>; 4] = [
            // The order and the sequence is essential.
            // It is used to correlate the revision numbers in manifest.
            Arc::new(Trivial::<bool>::default()),
            Arc::new(Trivial::<u64>::default()),
            Arc::new(Trivial::<Box<str>>::default()),
            Arc::new(TextIndex::default()),
        ];
        EngineBuilderWithIndices {
            processor,
            indices: defaults
                .into_iter()
                .map(|index| (index.id().into(), index))
                .collect(),
        }
    }

    /// Add a boolean index to the engine's
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withBooleanIndex")
    )]
    pub fn with_boolean_index(self) -> EngineBuilderWithIndices {
        self.with_index(Trivial::<bool>::default())
    }

    /// Add an integer index to the engine's
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withIntegerIndex")
    )]
    pub fn with_integer_index(self) -> EngineBuilderWithIndices {
        self.with_index(Trivial::<u64>::default())
    }

    /// Add a text index to the engine's
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withTagIndex")
    )]
    pub fn with_tag_index(self) -> EngineBuilderWithIndices {
        self.with_index(Trivial::<Box<str>>::default())
    }

    /// Add a text index to the engine's
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withTextIndex")
    )]
    pub fn with_text_index(self) -> EngineBuilderWithIndices {
        self.with_index(TextIndex::default())
    }

    /// Shortcut to finish the build with defaults and produce an engine
    pub fn build(self) -> Engine {
        self.with_default_indices().build()
    }
}

/// Each index can only be added once to the engine.
/// This error is returned on attempts to configure the same index multiple times.
#[derive(Debug)]
#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
pub struct DuplicateIndexIdError(Box<str>);
impl std::error::Error for DuplicateIndexIdError {}
impl std::fmt::Display for DuplicateIndexIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "An index with the same ID has already been added: {:?}",
            self.0
        )
    }
}

impl EngineBuilderWithIndices {
    /// Add engine's index
    pub fn with_index(
        mut self,
        index: impl 'static + Index,
    ) -> Result<Self, DuplicateIndexIdError> {
        let id = index.id();
        match self.indices.entry(id.into()) {
            Entry::Vacant(entry) => {
                entry.insert(Arc::new(index));
                Ok(self)
            }
            Entry::Occupied(_) => {
                // cannot add the same index multiple times
                Err(DuplicateIndexIdError(id.into()))
            }
        }
    }
}

#[cfg_attr(feature = "wasm-bindgen", wasm_bindgen::prelude::wasm_bindgen)]
impl EngineBuilderWithIndices {
    /// Add a boolean index to the engine's
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withBooleanIndex")
    )]
    pub fn with_boolean_index(self) -> Result<Self, DuplicateIndexIdError> {
        self.with_index(Trivial::<bool>::default())
    }

    /// Add an integer index to the engine's
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withIntegerIndex")
    )]
    pub fn with_integer_index(self) -> Result<Self, DuplicateIndexIdError> {
        self.with_index(Trivial::<u64>::default())
    }

    /// Add a text index to the engine's
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withTagIndex")
    )]
    pub fn with_tag_index(self) -> Result<Self, DuplicateIndexIdError> {
        self.with_index(Trivial::<Box<str>>::default())
    }

    /// Add a text index to the engine's
    #[cfg_attr(
        feature = "wasm-bindgen",
        wasm_bindgen::prelude::wasm_bindgen(js_name = "withTextIndex")
    )]
    pub fn with_text_index(self) -> Result<Self, DuplicateIndexIdError> {
        self.with_index(TextIndex::default())
    }

    /// Finish the build and produce an engine
    pub fn build(self) -> Engine {
        let Self { processor, indices } = self;
        Engine::new(processor, indices)
    }
}
