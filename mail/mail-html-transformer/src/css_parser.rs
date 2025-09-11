#[cfg(test)]
#[path = "tests/css_parser.rs"]
mod tests;

use lightningcss::error::Error as LightningError;
use lightningcss::stylesheet::{ParserOptions, StyleAttribute, StyleSheet};

/// Parses a CSS stylesheet with preprocessing to handle malformed CSS and error recovery.
#[allow(clippy::ptr_arg)]
pub fn parse_stylesheet(
    css: &mut String,
) -> Result<StyleSheet, LightningError<lightningcss::error::ParserError>> {
    StyleSheet::parse(
        css,
        ParserOptions {
            error_recovery: true,
            ..Default::default()
        },
    )
}

/// Parses a CSS style attribute with error recovery.
#[allow(clippy::ptr_arg)]
pub fn parse_style_attribute(
    css: &mut String,
) -> Result<StyleAttribute, LightningError<lightningcss::error::ParserError>> {
    StyleAttribute::parse(
        css,
        ParserOptions {
            error_recovery: true,
            ..Default::default()
        },
    )
}
