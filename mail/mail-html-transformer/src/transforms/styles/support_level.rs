use std::sync::LazyLock;

use html5ever::local_name;
use itertools::Itertools;
use kuchikiki::NodeRef;
use regex::Regex;
use sha1_checked::Sha1;

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
// Used SHA1-checked algorithm.
// When adding a new item, please use `CollisionResult::has_collision()` method
#[allow(clippy::unreadable_literal)]
const LIST_OF_UNTRUSTED_SENDERS: &[&[u8]] = &[
    // test@pm.me
    &[
        0x99, 0x0, 0x6c, 0x5f, 0xa2, 0x55, 0x16, 0xec, 0xe9, 0xdd, 0x9b, 0xb7, 0xfe, 0x0a, 0x80,
        0x15, 0xf4, 0x0, 0x3d, 0x2c,
    ],
    // List of unhashed items kept in jira comment ET-3178
    &[
        0xf6, 0xb2, 0x2c, 0x43, 0xff, 0x62, 0xa3, 0xa2, 0xda, 0x77, 0xdc, 0xab, 0x31, 0x8d, 0x51,
        0x9a, 0x18, 0xf3, 0x2f, 0xa4,
    ],
    &[
        0x39, 0xfc, 0xb0, 0xc6, 0x68, 0x1f, 0x4f, 0x3f, 0xdd, 0x16, 0x29, 0xa2, 0x33, 0xde, 0x92,
        0x49, 0x36, 0x28, 0xde, 0x2d,
    ],
    &[
        0x6c, 0x79, 0xaf, 0xc2, 0x55, 0xaa, 0xed, 0xab, 0x86, 0x11, 0x25, 0x7c, 0x91, 0xa2, 0xfd,
        0xc3, 0x8d, 0xc7, 0xeb, 0x70,
    ],
    &[
        0x74, 0x6d, 0x63, 0xbf, 0xed, 0x99, 0xf5, 0x58, 0xda, 0x4d, 0x66, 0x97, 0xd2, 0x8c, 0x33,
        0xc2, 0x2a, 0x22, 0xf9, 0xd2,
    ],
    &[
        0x48, 0x37, 0xf3, 0xec, 0x50, 0xc8, 0xa7, 0x9d, 0x49, 0xc6, 0xef, 0xcf, 0xe7, 0x86, 0xed,
        0xcc, 0xb7, 0x79, 0x85, 0x5d,
    ],
    &[
        0xe9, 0xe5, 0x19, 0x77, 0xcc, 0xf0, 0x92, 0x4c, 0x89, 0x2c, 0x32, 0x83, 0xc2, 0x8e, 0xc3,
        0xc6, 0xd2, 0xa9, 0x2c, 0xf3,
    ],
    &[
        0x2c, 0x3b, 0x9a, 0xd7, 0xac, 0x87, 0xb1, 0xf2, 0x36, 0xfc, 0x1e, 0xbe, 0xdd, 0x9, 0x39,
        0x9, 0x70, 0x88, 0xe5, 0x3f,
    ],
];

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

        let result = Sha1::try_digest(sender.as_bytes());
        let hash = result.hash();
        let hash = hash.as_slice();
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
