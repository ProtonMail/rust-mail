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
//! println!("{}",output);
//! ```
//!

use html5ever::tendril::TendrilSink;
use kuchikiki::NodeRef;
use message_detector::SplitDoc;
use sanitizer::StripStyleSheets;
use std::fmt::{Display, Formatter};
use std::io::Read;
use transforms::{ColorMode, keep_spaces_and_escape_gt_and_lt, styles::BrowserCapabilities};

// NOTE: each new transformation pass should be its own module.
pub mod css_parser;
pub mod ios;
pub mod message_detector;
pub mod proton_schemes;
pub mod remote_content;
pub mod sanitizer;
pub mod transforms;
pub mod utm;

mod html2text;

use crate::replace_inner::InvalidSelectorError;
use crate::transforms::styles::{IncludeFullStaticCss, InjectDarkModeOptions};
pub use html2text::Html2TextOptions;

mod replace_inner;
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

    /// This extracts innerHTML of `<body>`
    /// element
    #[must_use]
    pub fn extract_body(&self) -> String {
        let Ok(body) = self.document.select_first("body") else {
            return String::new();
        };

        inner_html(body.as_node())
    }

    /// Strip HTML links of UTM tracking codes.
    ///
    /// See [`utm::strip()`] for more details.
    /// Returns how many tracking codes it removed.
    #[tracing::instrument(skip_all)]
    pub fn strip_utm(&mut self) -> u64 {
        utm::strip(self.document.clone())
    }

    /// Disables remote content.
    ///
    /// See [`remote_content::disable_remote_content()`] for more details.
    #[tracing::instrument(skip_all)]
    pub fn disable_content(&mut self, no_remote: bool, no_embedded: bool) -> (u64, u64) {
        remote_content::disable_content(&self.document, no_remote, no_embedded)
    }

    /// Transform image URLs from HTTP/HTTPS to proton-http/proton-https schemes.
    #[tracing::instrument(skip_all)]
    pub fn transform_to_proton_schemes(&mut self) -> u64 {
        proton_schemes::transform_to_proton_schemes(self.document.clone())
    }

    /// Transform image URLs from proton-http/proton-https schemes back to HTTP/HTTPS.
    #[tracing::instrument(skip_all)]
    pub fn transform_from_proton_schemes(&mut self) -> u64 {
        proton_schemes::transform_from_proton_schemes(self.document.clone())
    }

    /// If true, inject metadata for iOS web view.
    ///
    /// See [`ios::inject_content_size()`] for more details.
    #[tracing::instrument(skip_all)]
    pub fn inject_ios_content_size(&mut self) {
        ios::inject_content_size(self.document.clone());
    }

    /// This function removes disallowed tags and attributes.
    ///
    /// See [`sanitizer::strip_whitelist`] for more details.
    #[tracing::instrument(skip_all)]
    pub fn strip_whitelist(&mut self, strip_style_sheets: StripStyleSheets) -> u64 {
        sanitizer::strip_whitelist(self.document.clone(), strip_style_sheets)
    }

    /// Reverts dark mode injection in inline attributes.
    #[tracing::instrument(skip_all)]
    pub fn revert_dark_mode_in_inline_attributes(&mut self) {
        transforms::styles::revert_dark_mode_in_inline_attributes(&self.document);
    }

    /// This function adds dark mode support. This fails if the html doesn't have a head tag.
    #[tracing::instrument(skip_all)]
    pub fn inject_dark_mode(
        &mut self,
        sender: &str,
        mode: ColorMode,
        capabilities: BrowserCapabilities,
        include_full_static_css: IncludeFullStaticCss,
        trusted_senders: &[&str],
    ) {
        transforms::styles::inject_root_selector_to_html(&self.document);
        transforms::styles::inject_dark_mode(
            self.document.clone(),
            self.document.clone(),
            InjectDarkModeOptions {
                sender: Some(sender),
                mode,
                capabilities,
                root_selector: "[data-protonmail-message]".to_owned(),
                include_full_static_css,
                trusted_senders,
            },
        );
    }

    /// This function adds dark mode support. It does modify original body only in the context
    /// of removing `!important` flag from styles and attributes.
    ///
    /// Supplement CSS are not injected, instead the function returns the head of the new document.
    pub fn inject_dark_mode_to_another_target(&mut self, options: InjectDarkModeOptions) -> String {
        use html5ever::namespace_url;
        let source = self.document.clone();
        let target = NodeRef::new_document();
        let head = NodeRef::new_element(
            html5ever::QualName::new(
                None,
                html5ever::ns!(html),
                html5ever::LocalName::from("head"),
            ),
            vec![],
        );
        target.append(head.clone());

        transforms::styles::inject_dark_mode(source, target.clone(), options);
        inner_html(&head)
    }

    ///
    /// See [`transforms::add_noreferrer`] for more details.
    ///
    /// This requires an [`InsertLinkToken`]
    #[tracing::instrument(skip_all)]
    pub fn add_noreferrer(&mut self) -> InsertLinkToken {
        transforms::add_noreferrer(self.document.clone());
        InsertLinkToken(())
    }

    /// Inserts `<a>` elements in plain text links
    #[tracing::instrument(skip_all)]
    pub fn insert_links(&mut self, _token: InsertLinkToken) {
        transforms::insert_links(self.document.clone());
    }

    /// Removes the blockquote from the html
    #[tracing::instrument(skip_all)]
    pub fn strip_blockquote(&mut self) -> bool {
        message_detector::strip_blockquote(self.document().clone())
    }

    /// Try to locate and extract the eventual blockquote present in the document no matter the expeditor of the mail
    #[tracing::instrument(skip_all)]
    pub fn extract_blockquote(&mut self) -> SplitDoc {
        message_detector::locate_blockquote(self.document().clone())
    }

    /// Moves every `<style>` from `<head>` into `<body>`.
    pub fn move_styles_to_body(&mut self) {
        transforms::move_styles_to_body(self.document().clone());
    }

    pub fn to_plain_text(&self, options: Html2TextOptions) -> Result<String, ::html2text::Error> {
        let html = self.to_string();
        Self::html2text_str(&html, options)
    }

    pub fn html2text(
        reader: impl Read,
        options: Html2TextOptions,
    ) -> Result<String, ::html2text::Error> {
        html2text::html2text(reader, options)
    }

    pub fn html2text_str(
        reader: &str,
        options: Html2TextOptions,
    ) -> Result<String, ::html2text::Error> {
        let cursor = std::io::Cursor::new(reader);
        Self::html2text(cursor, options)
    }

    /// See [`replace_inner::replace_inner_div`] for more details.
    pub fn replace_inner_div(
        &self,
        div_class: &str,
        replacement: &str,
    ) -> Result<(), InvalidSelectorError> {
        replace_inner::replace_inner_div(&self.document, div_class, replacement)
    }
}

fn inner_html(node: &NodeRef) -> String {
    use std::fmt::Write;

    let mut result = String::new();
    for child in node.children() {
        write!(result, "{}", child.to_string()).expect("writing to string");
    }
    result
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
