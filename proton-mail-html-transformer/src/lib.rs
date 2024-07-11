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
//!
//! let html = "..";
//!
//! // Create the transformed.
//! let options = Options {
//!     strip_utm: true,
//!     inject_ios_content_size: false
//! };
//! let transformer = Transformer::new(options);
//!
//! // Retrieve the parsed and transformed html.
//! let parsed = transformer.transform(html).unwrap();
//! // Convert back to textual representation
//! let transformed_html = parsed.to_string();
//!
//! ```
//!

use html5ever::tendril::TendrilSink;
use kuchikiki::NodeRef;

// NOTE: each new transformation pass should be its own module.
mod ios;
pub mod utm;

/// Control the transformer behavior by selecting which transformations to apply.
///
/// By default, other than the sanitization stage, all the remaining stages need to
/// be enabled manually.
#[derive(Default)]
pub struct Options {
    /// If true, enable stripping of UTM tracking codes.
    ///
    /// See [`utm::strip()`] for more details.
    pub strip_utm: bool,
    /// If true, inject metadata for iOS web view.
    ///
    /// See [`ios::inject_content_size()`] for more details.
    pub inject_ios_content_size: bool,
}

/// Errors that may occur during transformation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Error occurred during UTM pass.
    #[error("Utm: {0}")]
    Utm(#[from] utm::Error),
    /// Error occurred during iOS pass.
    #[error("iOS: {0}")]
    Ios(#[from] ios::Error),
}

/// HTML content transformer.
///
/// Behavior of the transformer is controlled via the [`Options`] type.
pub struct Transformer {
    /// Transform options.
    options: Options,
}

impl Transformer {
    /// Create a new [`Transformer`] with the given `options`.
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
    ///
    /// Returns error if any of the transformation passes fail.
    pub fn transform(&self, document: &str) -> Result<NodeRef, Error> {
        let document = kuchikiki::parse_html().one(document);

        self.transform_parsed(document)
    }

    /// Transform a previously parsed HTML `document`.
    ///
    /// This method returns the parsed HTML content in a format that is suitable for further
    /// transformations and/or inspection.
    ///
    /// # Errors
    ///
    /// Returns error if any of the transformation passes fail.
    pub fn transform_parsed(&self, document: NodeRef) -> Result<NodeRef, Error> {
        if self.options.strip_utm {
            utm::strip(&document)?;
        }

        if self.options.inject_ios_content_size {
            ios::inject_content_size(&document)?;
        }

        Ok(document)
    }
}
