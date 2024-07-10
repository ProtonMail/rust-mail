//! HTML content transformer for proton mail applications.
//!
//! The transformer contains passes which mainly focus on preserving the privacy of the reader
//! and removing dangerous content. These can range from stripping advertising identifiers to
//! removing embedded tracker images.
//!
//! Some of the transformer passes may also available as standalone code unit.
//!
//! # Example
//!
//! ```
//! use proton_mail_html_transformer::{Options, Transformer};
//! let html = "..";
//! // Create the transformed.
//! let transformer = Transformer::new(Options::new().strip_utm());
//!
//! // Retrieve the parsed and transformed html.
//! let parsed = transformer.transform(html).unwrap();
//! // Convert back to textual representation
//! let transformed_html = parsed.to_string();
//!
//! // Equivalent step that directly converts to String.
//! let transformed_html = transformer.transform_to_string(html);
//! ```
//!

use html5ever::tendril::TendrilSink;
use kuchikiki::NodeRef;

// NOTE: each new transformation pass should be its own module.
pub mod utm;

// Re-export in order to access node type.
pub use kuchikiki;

/// Control the transformer behavior by selecting which transformations to apply.
///
/// By default, other than the sanitization stage, all the remaining stages need to
/// be enabled manually.
pub struct Options {
    strip_utm: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self::new()
    }
}
impl Options {
    /// Create a new instance.
    #[must_use]
    pub fn new() -> Self {
        Self { strip_utm: false }
    }

    /// Enable stripping of UTM tracking codes.
    ///
    /// See [`utm::strip()`] for more details.
    #[must_use]
    pub fn strip_utm(mut self) -> Self {
        self.strip_utm = true;
        self
    }
}

/// Errors that may occur during transformation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Error occurred during UTM pass.
    #[error("Utm: {0}")]
    Utm(#[from] utm::Error),
}

/// HTML content transformer.
///
/// Behavior of the transformer is controlled via the [`Options`] type.
pub struct Transformer {
    options: Options,
}

impl Transformer {
    /// Create a new transform with the given `options`.
    #[must_use]
    pub fn new(options: Options) -> Self {
        Self { options }
    }

    /// Transform an HTML `document`.
    ///
    /// This method returns the parsed HTML content in a format that is suitable for further
    /// transformations and/or inspection.
    ///
    /// # Errors
    /// Returns error if any of the transformation passes fail.
    pub fn transform(&self, document: &str) -> Result<NodeRef, Error> {
        let document = kuchikiki::parse_html().one(document);

        self.transform_parsed(document)
    }

    /// Transform an HTML `document`.
    ///
    /// This method returns the parsed HTML content as an HTML string.
    ///
    /// # Errors
    /// Returns error if any of the transformation passes fail.
    pub fn transform_to_string(&self, str: &str) -> Result<String, Error> {
        self.transform(str).map(|v| v.to_string())
    }

    /// Transform a previously parsed HTML `document`.
    ///
    /// This method returns the parsed HTML content in a format that is suitable for further
    /// transformations and/or inspection.
    ///
    /// # Errors
    /// Returns error if any of the transformation passes fail.
    pub fn transform_parsed(&self, document: NodeRef) -> Result<NodeRef, Error> {
        if self.options.strip_utm {
            utm::strip(&document)?;
        }

        Ok(document)
    }
}
