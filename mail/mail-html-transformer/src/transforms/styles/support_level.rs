use std::{
    hash::{DefaultHasher, Hash, Hasher},
    sync::LazyLock,
};

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

// Hashed list of senders that we want to override dark mode even though messages
// sent by them are containing indicators that they support dark mode.
#[allow(clippy::unreadable_literal)]
const LIST_OF_UNTRUSTED_SENDERS: &[u64] = &[
    1756601761911002984, // test@pm.me
    // List of items kept in jira comment ET-3178
    9520100162928875237,
    12905929706031470708,
    6992208906078590969,
    6193077142137444132,
    7306178817655991380,
    2220191526828288093,
    4716266425355125727,
];

fn hash_sender(sender: &str) -> u64 {
    // TODO: This hasher is not stable. It might break upon over releases of Rust.
    // In that case we will have to re-hash the list of emails.
    let mut state = DefaultHasher::new();
    sender.hash(&mut state);
    state.finish()
}

impl DarkStyleSupportLevel {
    pub(crate) fn new_for_plaintext(mode: ColorMode, capabilities: BrowserCapabilities) -> Self {
        if mode == ColorMode::LightMode && !capabilities.supports_dark_mode_via_media_query {
            return Self::NoDarkMode;
        }

        Self::Native
    }

    /// If the sender is in the exception list, we will override dark mode for them.
    fn is_sender_untrusted(sender: Option<&str>) -> bool {
        let Some(sender) = sender else { return false };

        let hash = hash_sender(sender);
        LIST_OF_UNTRUSTED_SENDERS.contains(&hash)
    }

    /// * `sender` is the email address of the sender. Example: `test@pm.me`
    pub(crate) fn new_for_html(
        sender: Option<&str>,
        mode: ColorMode,
        document: &NodeRef,
        capabilities: BrowserCapabilities,
    ) -> Self {
        // If browser supports `@media` query then even in the light mode we want to process
        // styles for the dark mode. Because theme can change without reloading
        if mode == ColorMode::LightMode && !capabilities.supports_dark_mode_via_media_query {
            return Self::NoDarkMode;
        }

        // Some senders are not able to handle dark mode properly.
        // We will override dark mode for them.
        if Self::is_sender_untrusted(sender) {
            return Self::Injected;
        }

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

    #[test_case(None, r#"<html><head> <meta name="supported-color-schemes" content="[light? || dark? || <ident>?]* || only?"></head><body></body></html>"#, ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Native ; "case 1")]
    #[test_case(None, r#"<html><head> <meta name="color-scheme" content="[light? || dark? || <ident>?]* || only?"></head><body></body></html>"#, ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Native ; "case 2")]
    #[test_case(None, "<html><head> <style>:root{color-scheme: light dark;}</style></head><body></body></html>", ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Native ; "case 3")]
    #[test_case(None, "<html><head></head><body> <table> </table></body></html>", ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Injected ; "case 4")]
    #[test_case(None, "<html><head></head><body> <div> <div> <div> <div> <div> <div> <div> <div> <div> <div> <div> <div> <div> <div> hi </div></div></div></div></div></div></div></div></div></div></div></div></div></div></body></html>", ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Injected ; "case 5")]
    #[test_case(None, "<html><head></head><body> <span> a</span></body></html>", ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Injected ; "case 6")]
    #[test_case(None, "", ColorMode::LightMode, DOESNT_SUPPORT_MEDIA => DarkStyleSupportLevel::NoDarkMode ; "case 7")]
    #[test_case(None, "", ColorMode::LightMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Injected ; "case 8")]
    #[test_case(Some("test@pm.me"), r#"<html><head> <meta name="supported-color-schemes" content="[light? || dark? || <ident>?]* || only?"></head><body></body></html>"#, ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Injected ; "case 9")]
    fn test_support_level(
        sender: Option<&str>,
        input: &str,
        mode: ColorMode,
        capabilities: BrowserCapabilities,
    ) -> DarkStyleSupportLevel {
        let html = kuchikiki::parse_html().one(input);

        DarkStyleSupportLevel::new_for_html(sender, mode, &html, capabilities)
    }
}
