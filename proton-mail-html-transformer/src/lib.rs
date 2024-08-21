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
//! let html = "<html>..</html>";
//!
//! let transformed_html = Transformer::new(html)
//!   .strip_utm() // Strip utm codes.
//!   .to_string();// Convert back to textual representation
//! ```
//!

use html5ever::tendril::TendrilSink;
use kuchikiki::NodeRef;
use message_detector::SplitDoc;
use std::fmt::{Display, Formatter};

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
/// Each pass is exposed as separate method. Some of the passes are destructive in nature, while
/// others can be undone. See each method for more details.
#[derive(Debug, Clone)]
pub struct Transformer {
    ///Parsed document.
    document: NodeRef,
    insert_links_called: bool,
}

impl Transformer {
    /// Create a new [`Transformer`] with the given `document` HTML string.
    #[must_use]
    pub fn new(document: &str) -> Self {
        let document = kuchikiki::parse_html().one(document);
        Self {
            document,
            insert_links_called: false,
        }
    }

    /// Create a new [`Transformer`] with a previously parsed `document`.
    #[must_use]
    pub fn with_parsed(document: NodeRef) -> Self {
        Self {
            document,
            insert_links_called: false,
        }
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
    ///
    /// # Remarks
    ///
    /// This is a destructive operation and can not be undone.
    pub fn strip_utm(&mut self) -> u64 {
        utm::strip(self.document.clone())
    }

    /// Disables remote content.
    ///
    /// See [`remote_content::disable_remote_content()`] for more details.
    ///
    /// # Remarks
    ///
    /// This is a non-destructive operation and can be undone with [`enable_remote_content()`].
    ///
    pub fn disable_remote_content(&mut self) -> &mut Self {
        remote_content::disable_remote_content(&self.document);
        self
    }

    /// Enables remote content.
    ///
    /// See [`remote_content::undo_disable_remote_content()`] for more details.
    ///
    /// # Remarks
    ///
    /// This is a non-destructive operation and can be undone with [`disable_remote_content()`].
    pub fn enable_remote_content(&mut self) -> &mut Self {
        remote_content::undo_disable_remote_content(&self.document);
        self
    }

    /// If true, inject metadata for iOS web view.
    ///
    /// See [`ios::inject_content_size()`] for more details.
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
    pub fn strip_whitelist(&mut self) -> u64 {
        sanitizer::strip_whitelist(self.document.clone())
    }

    /// This function adds dark mode support. This fails if the html doesn't have a head tag.
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
    ///
    /// # Panics
    ///
    /// For performance reasons call this before [`Transformer::insert_links`]
    pub fn add_noreferrer(&mut self) -> &mut Self {
        assert!(
            !self.insert_links_called,
            "For performance reasons call this before `Transformer::insert_links`"
        );
        transforms::add_noreferrer(self.document.clone());
        self
    }

    /// Proxies all images through proton's proxy.
    ///
    /// # Remarks
    ///
    /// This is a destructive operation and can not be undone.
    pub fn proxy_images(&mut self, user_session_id: &str) -> &mut Self {
        transforms::proxy_images(self.document(), user_session_id);
        self
    }

    /// Inserts `<a>` elements in plain text links
    ///
    /// # Remarks
    ///
    /// This is a destructive operation and can not be undone.
    pub fn insert_links(&mut self) -> &mut Self {
        self.insert_links_called = true;
        transforms::insert_links(self.document.clone());
        self
    }

    /// Removes the blockquote from the html
    ///
    /// # Remarks
    ///
    /// This is a destructive operation and can not be undone.
    pub fn strip_blockquote(&mut self) -> bool {
        message_detector::strip_blockquote(self.document().clone())
    }

    /// Try to locate and extract the eventual blockquote present in the document no matter the expeditor of the mail
    ///
    /// # Remarks
    ///
    /// This is a destructive operation and can not be undone.
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
