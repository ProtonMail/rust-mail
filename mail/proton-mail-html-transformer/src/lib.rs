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
//! use proton_mail_html_transformer::Transformer;
//!
//! let html = "..";
//!
//! let mut transformer = Transformer::new(html);
//!
//! // Strip utm codes.
//! transformer.strip_utm().unwrap();
//!
//! // Convert back to textual representation
//! let transformed_html = transformer.to_string();
//!
//! ```
//!

use html5ever::tendril::TendrilSink;
use kuchikiki::NodeRef;
use std::fmt::{Display, Formatter};

// NOTE: each new transformation pass should be its own module.
mod ios;
mod remote_content;
pub mod utm;

/// Errors that may occur during transformation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Error occurred during UTM pass.
    #[error("Utm: {0}")]
    Utm(#[from] utm::Error),
    /// Error occurred during iOS pass.
    #[error("iOS: {0}")]
    Ios(#[from] ios::Error),
    /// Error occurred during Remote Content pass.
    #[error("Remote Content: {0}")]
    RemoteContent(#[from] remote_content::Error),
}

/// HTML content transformer.
///
/// This type contains a couple of passes which transform the parsed HTML in order to sanitize
/// and/or enhance the privacy of the user.
///
/// Each pass is exposed as separate method. Some of the passes are destructive in nature, while
/// others can be undone. See each method for more details.
#[derive(Clone)]
pub struct Transformer {
    ///Parsed document.
    document: NodeRef,
}

impl Transformer {
    /// Create a new [`Transformer`] with the given `document` HTML string.
    #[must_use]
    pub fn new(document: &str) -> Self {
        let document = kuchikiki::parse_html().one(document);
        Self { document }
    }

    /// Create a new [`Transformer`] with a previously parsed `document`.
    #[must_use]
    pub fn with_parsed(document: NodeRef) -> Self {
        Self { document }
    }

    /// Access the parsed document.
    #[must_use]
    pub fn document(&self) -> &NodeRef {
        &self.document
    }

    /// Strip HTML links of UTM tracking codes.
    ///
    /// See [`utm::strip()`] for more details.
    ///
    /// # Remarks
    ///
    /// This is a destructive operation and can not be undone.
    ///
    /// # Errors
    ///
    /// Returns errors if the pass failed.
    pub fn strip_utm(&mut self) -> Result<(), utm::Error> {
        utm::strip(&self.document)
    }

    /// Disables remote content.
    ///
    /// See [`remote_content::disable_remote_content()`] for more details.
    ///
    /// # Remarks
    ///
    /// This is a non-destructive operation and can be undone with [`enable_remote_content()`].
    ///
    /// # Errors
    ///
    /// Returns errors if the pass failed.
    pub fn disable_remote_content(&mut self) -> Result<(), remote_content::Error> {
        remote_content::disable_remote_content(&self.document)
    }

    /// Enables remote content.
    ///
    /// See [`remote_content::undo_disable_remote_content()`] for more details.
    ///
    /// # Remarks
    ///
    /// This is a non-destructive operation and can be undone with [`disable_remote_content()`].
    ///
    /// # Errors
    ///
    /// Returns errors if the pass failed.
    pub fn enable_remote_content(&mut self) -> Result<(), remote_content::Error> {
        remote_content::undo_disable_remote_content(&self.document)
    }

    /// If true, inject metadata for iOS web view.
    ///
    /// See [`ios::inject_content_size()`] for more details.
    ///
    /// # Errors
    ///
    /// Returns errors if the pass failed.
    pub fn inject_ios_content_size(&mut self) -> Result<(), Error> {
        Ok(ios::inject_content_size(&self.document)?)
    }
}

impl Display for Transformer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.document.to_string())
    }
}
