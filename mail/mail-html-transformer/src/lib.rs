// https://github.com/rust-lang/rust-clippy/issues/13155 This lint is complaining about passing an Rc by value!
#![allow(clippy::needless_pass_by_value)]

//! HTML content transformer for proton mail applications.
//!
//! The transformer contains passes which mainly focus on preserving the privacy of the reader
//! and removing dangerous content. These can range from stripping advertising identifiers to
//! removing embedded tracker images.
//!
//! Some of the transformer passes may also available as standalone code unit.
//!
//! Note:
//!
//! Transformer does not respect any whitespace formatting, HTML nodes won't be beautified.
//!
//! # Example
//!
//! ```
//! use proton_mail_html_transformer::Transformer;
//!
//! let input = r#"
//! <html>
//!     <body>
//!         <a href="https://ads.com?utm_source=tracker">bar</a>
//!     </body>
//! </html>
//! "#;
//!
//!
//! let mut transformer = Transformer::new(input);
//! transformer.strip_utm();
//! let output = transformer.to_string();
//!
//! let expected = r#"<html><head></head><body>
//!         <a href="https://ads.com/">bar</a>
//!    
//!
//! </body></html>"#;
//! assert_eq!(expected, output);
//! ```
//!

use html5ever::tendril::TendrilSink;
use kuchikiki::NodeRef;
use message_detector::SplitDoc;
use std::fmt::{Display, Formatter};
use transforms::keep_spaces_and_escape_gt_and_lt;

// NOTE: each new transformation pass should be its own module.
pub mod ios;
pub mod message_detector;
pub mod remote_content;
pub mod sanitizer;
pub mod transforms;
pub mod utm;

#[cfg(test)]
#[path = "tests/lib.rs"]
mod tests;

/// HTML content transformer.
///
/// This type contains a couple of passes which transform the parsed HTML in order to sanitize
/// and/or enhance the privacy of the user.
///
/// Each pass is exposed as separate method.
#[derive(Debug, Clone)]
pub struct Transformer {
    /// Parsed document.
    document: NodeRef,
}

/// This exists because `add_noreferrer` should be called before `insert_links` for performance
/// reasons.
#[derive(Clone, Copy)]
pub struct InsertLinkToken(());

impl Transformer {
    /// Create a new [`Transformer`] with the given `document` HTML string.
    #[must_use]
    pub fn new(document: &str) -> Self {
        let document = kuchikiki::parse_html().one(document);
        Self { document }
    }

    /// Create a new [`Transformer`] with the given plain text string.
    #[must_use]
    pub fn new_text_plain(plain_text: &str) -> Self {
        let document = keep_spaces_and_escape_gt_and_lt(plain_text);
        let document = kuchikiki::parse_html().one(document.as_str());
        Self { document }
    }

    /// Create a new [`Transformer`] with a previously parsed `document`.
    #[must_use]
    pub fn with_parsed(document: NodeRef) -> Self {
        Self { document }
    }

    /// Access the parsed document.
    #[must_use]
    pub fn document(&self) -> NodeRef {
        self.document.clone()
    }

    /// Strip HTML links of UTM tracking codes.
    ///
    /// See [`utm::strip()`] for more details.
    /// Returns how many tracking codes it removed.
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub fn strip_utm(&mut self) -> u64 {
        utm::strip(self.document.clone())
    }

    /// Disables remote content.
    ///
    /// See [`remote_content::disable_remote_content()`] for more details.
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub fn disable_content(&mut self, no_remote: bool, no_embedded: bool) -> (u64, u64) {
        remote_content::disable_content(&self.document, no_remote, no_embedded)
    }

    /// If true, inject metadata for iOS web view.
    ///
    /// See [`ios::inject_content_size()`] for more details.
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub fn inject_ios_content_size(&mut self) {
        ios::inject_content_size(self.document.clone());
    }

    /// This function removes disallowed tags and attributes.
    ///
    /// See [`sanitizer::strip_whitelist`] for more details.
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub fn strip_whitelist(&mut self) -> u64 {
        sanitizer::strip_whitelist(self.document.clone())
    }

    /// This function adds dark mode support. This fails if the html doesn't have a head tag.
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub fn inject_style(&mut self) {
        transforms::inject_style(self.document.clone());
    }

    ///
    /// See [`transforms::add_noreferrer`] for more details.
    ///
    /// This requires an [`InsertLinkToken`]
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub fn add_noreferrer(&mut self) -> InsertLinkToken {
        transforms::add_noreferrer(self.document.clone());
        InsertLinkToken(())
    }

    /// Inserts `<a>` elements in plain text links
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub fn insert_links(&mut self, _token: InsertLinkToken) {
        transforms::insert_links(self.document.clone());
    }

    /// Removes the blockquote from the html
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub fn strip_blockquote(&mut self) -> bool {
        message_detector::strip_blockquote(self.document().clone())
    }

    /// Try to locate and extract the eventual blockquote present in the document no matter the expeditor of the mail
    #[tracing::instrument(level = tracing::Level::DEBUG, skip_all)]
    pub fn extract_blockquote(&mut self) -> SplitDoc {
        message_detector::locate_blockquote(self.document().clone())
    }
}

// WARN: This is vulnerable to malicious HTMLs with very deeply nested tags.
impl Display for Transformer {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.document.to_string())
    }
}

#[cfg(test)]
mod integration_tests {
    // I get a really strange linker error if I `import cpuprofiler as _`.
    // TODO: Report this bug to rustc.
    // use cpuprofiler as _;
    use criterion as _;
    use pprof as _;
}
