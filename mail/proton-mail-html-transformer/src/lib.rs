// https://github.com/rust-lang/rust-clippy/issues/13155
// This lint is complaining about passing an Rc by value!
#![allow(clippy::needless_pass_by_value)]

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
pub mod ios;
pub mod remote_content;
pub mod sanitizer;
pub mod transforms;
pub mod utm;

/// Errors that may occur during transformation.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Error occurred during UTM pass.
    #[error("Utm: {0}")]
    Utm(#[from] utm::Error),
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
#[derive(Debug, Clone)]
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
    pub fn strip_utm(&mut self) -> Result<&mut Self, utm::Error> {
        utm::strip(self.document.clone())?;
        Ok(self)
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
    pub fn disable_remote_content(&mut self) -> Result<&mut Self, remote_content::Error> {
        remote_content::disable_remote_content(&self.document)?;
        Ok(self)
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
    pub fn enable_remote_content(&mut self) -> Result<&mut Self, remote_content::Error> {
        remote_content::undo_disable_remote_content(&self.document)?;
        Ok(self)
    }

    /// If true, inject metadata for iOS web view.
    ///
    /// See [`ios::inject_content_size()`] for more details.
    ///
    /// # Errors
    ///
    /// Returns errors if the pass failed.
    pub fn inject_ios_content_size(&mut self) -> &mut Self {
        ios::inject_content_size(self.document.clone());
        self
    }

    /// This function removes disallowed tags and attributes.
    ///
    /// See [`sanitizer::strip_whitelist`] for more details.
    ///
    /// # Remarks
    ///
    /// This is a destructive operation and can not be undone.
    pub fn strip_whitelist(&mut self) -> &mut Self {
        sanitizer::strip_whitelist(self.document.clone());
        self
    }

    /// This function adds dark mode support. This fails if the html doesn't have a head tag.
    ///
    /// See [`transforms::inject_style`] for more details.
    ///
    /// # Remarks
    ///
    /// This is a destructive operation and can not be undone.
    pub fn inject_style(&mut self) -> &mut Self {
        transforms::inject_style(self.document.clone());
        self
    }

    ///
    /// See [`transforms::add_noreferrer`] for more details.
    ///
    /// # Remarks
    ///
    /// This is a destructive operation and can not be undone.
    pub fn add_noreferrer(&mut self) -> &mut Self {
        transforms::add_noreferrer(self.document.clone());
        self
    }

    /// Inserts `<a>` elements in plain text links
    ///
    /// # Remarks
    ///
    /// This is a destructive operation and can not be undone.
    pub fn insert_links(&mut self) -> &mut Self {
        transforms::insert_links(self.document.clone());
        self
    }
}

impl Display for Transformer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.document.to_string())
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn pathologic_nested() {
        // This test includes a very deeply nested html that we can use for stack overflow
        // detection
        let doc = include_str!("../tests/htmls/nested.html");
        Transformer::new(doc)
            .strip_utm()
            .unwrap()
            .strip_whitelist()
            .inject_ios_content_size()
            .disable_remote_content()
            .unwrap()
            .inject_style()
            .add_noreferrer();
        // .to_string(); // https://github.com/servo/html5ever/issues/290
    }
}
