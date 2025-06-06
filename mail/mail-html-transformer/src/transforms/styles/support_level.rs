use std::sync::LazyLock;

use html5ever::local_name;
use itertools::Itertools;
use kuchikiki::NodeRef;
use regex::Regex;

use crate::transforms::ColorMode;

use super::capabilities::BrowserCapabilities;

/// Defines strategy of what to do the CSS in the Dark Mode.
///
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum DarkStyleSupportLevel {
    /// User forced light mode or we are in the light color scheme
    NoDarkMode,
    /// Message was probably designed for Light Mode, so we need to
    /// parse existing colors and override them.
    Injected,
    /// Message contains CSS definitions that indicate, that it can render in the
    /// dark mode natively without our intervention
    Native,
}

static COLOR_SCHEME_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"color-scheme:\s?\S{0,}\s?dark").unwrap());

impl DarkStyleSupportLevel {
    pub(crate) fn new_for_plaintext(mode: ColorMode, capabilities: BrowserCapabilities) -> Self {
        if mode == ColorMode::LightMode && !capabilities.supports_dark_mode_via_media_query {
            return Self::NoDarkMode;
        }

        Self::Native
    }

    pub(crate) fn new_for_html(
        mode: ColorMode,
        document: &NodeRef,
        capabilities: BrowserCapabilities,
    ) -> Self {
        // If browser supports `@media` query then even in the light mode we want to process
        // styles for the dark mode. Because theme can change without reloading
        if mode == ColorMode::LightMode && !capabilities.supports_dark_mode_via_media_query {
            return Self::NoDarkMode;
        }

        // TODO: Filter out sender based on hashed list
        // if provideDarkModeCss(sender) {
        //     return Self::ProtonSupport;
        // }

        // TODO: Replace with let chains after its stabilized
        //
        if let Ok(meta) = document.select_first(r#"meta[name="color-scheme"]"#) {
            if let Some(content) = meta.attributes.borrow().get(local_name!("content")) {
                if content.contains("dark") {
                    tracing::debug!("Message contains color-scheme meta tag with dark content");
                    return Self::Native;
                }
            }
        }

        if let Ok(meta) = document.select_first(r#"meta[name="supported-color-schemes"]"#) {
            if let Some(content) = meta.attributes.borrow().get(local_name!("content")) {
                if content.contains("dark") {
                    tracing::debug!(
                        "Message contains supported-color-schemes meta tag with dark content"
                    );
                    return Self::Native;
                }
            }
        }

        if let Ok(style) = document.select("style") {
            let content = style.map(|el| el.text_contents()).join("\n");
            // Message contains media query that suggests it can handle dark mode natively
            if COLOR_SCHEME_REGEX.is_match(&content) {
                tracing::debug!("Message contains color-scheme meta tag with dark content");
                return Self::Native;
            }
        }

        Self::Injected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use html5ever::tendril::TendrilSink;
    use test_case::test_case;

    const SUPPORTS_MEDIA: BrowserCapabilities = BrowserCapabilities {
        supports_dark_mode_via_media_query: true,
    };
    const DOESNT_SUPPORT_MEDIA: BrowserCapabilities = BrowserCapabilities {
        supports_dark_mode_via_media_query: false,
    };

    #[test_case(r#"<html><head> <meta name="supported-color-schemes" content="[light? || dark? || <ident>?]* || only?"></head><body></body></html>"#, ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Native ; "case 1")]
    #[test_case(r#"<html><head> <meta name="color-scheme" content="[light? || dark? || <ident>?]* || only?"></head><body></body></html>"#, ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Native ; "case 2")]
    #[test_case("<html><head> <style>:root{color-scheme: light dark;}</style></head><body></body></html>", ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Native ; "case 3")]
    #[test_case("<html><head></head><body> <table> </table></body></html>", ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Injected ; "case 4")]
    #[test_case("<html><head></head><body> <div> <div> <div> <div> <div> <div> <div> <div> <div> <div> <div> <div> <div> <div> hi </div></div></div></div></div></div></div></div></div></div></div></div></div></div></body></html>", ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Injected ; "case 5")]
    #[test_case("<html><head></head><body> <span> a</span></body></html>", ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Injected ; "case 6")]
    #[test_case("", ColorMode::LightMode, DOESNT_SUPPORT_MEDIA => DarkStyleSupportLevel::NoDarkMode ; "case 7")]
    #[test_case("", ColorMode::LightMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Injected ; "case 8")]
    // TODO: Test this HTML with `test@pm.me` after we add support for exception-list
    // #"<html><head> <meta name="supported-color-schemes" content="[light? || dark? || <ident>?]* || only?"></head><body></body></html>"#
    fn test_support_level(
        input: &str,
        mode: ColorMode,
        capabilities: BrowserCapabilities,
    ) -> DarkStyleSupportLevel {
        let html = kuchikiki::parse_html().one(input);

        DarkStyleSupportLevel::new_for_html(mode, &html, capabilities)
    }
}
