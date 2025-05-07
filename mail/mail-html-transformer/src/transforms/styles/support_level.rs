use std::sync::LazyLock;

use html5ever::local_name;
use itertools::Itertools;
use kuchikiki::NodeRef;
use regex::Regex;

use crate::transforms::ColorMode;

use super::capabilities::BrowserCapabilities;

/// Defines strategy of what to do the CSS in the Dark Mode.
///
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
    pub(crate) fn new(
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
                    return Self::Native;
                }
            }
        }

        if let Ok(meta) = document.select_first(r#"meta[name="supported-color-schemes"]"#) {
            if let Some(content) = meta.attributes.borrow().get(local_name!("content")) {
                if content.contains("dark") {
                    return Self::Native;
                }
            }
        }

        if let Ok(style) = document.select("style") {
            let content = style.map(|el| el.text_contents()).join("\n");
            // Message contains media query that suggests it can handle dark mode natively
            if COLOR_SCHEME_REGEX.is_match(&content) {
                return Self::Native;
            }
        }

        Self::Injected
    }
}
