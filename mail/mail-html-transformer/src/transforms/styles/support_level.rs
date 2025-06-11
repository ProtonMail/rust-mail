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
    /// Message is sent by the trusted sender that supports dark mode natively.
    Native,
}

/// List of senders that we trust that they support dark mode on their own (with @media queries).
const LIST_OF_TRUSTED_SENDERS: &[&str] = &["test@pm.me"];

impl DarkStyleSupportLevel {
    pub(crate) fn new_for_plaintext(mode: ColorMode, capabilities: BrowserCapabilities) -> Self {
        if mode == ColorMode::LightMode && !capabilities.supports_dark_mode_via_media_query {
            return Self::NoDarkMode;
        }

        Self::Native
    }

    /// * `sender` is the email address of the sender. Example: `test@pm.me`
    pub(crate) fn new_for_html(
        sender: Option<&str>,
        mode: ColorMode,
        capabilities: BrowserCapabilities,
    ) -> Self {
        // If browser supports `@media` query then even in the light mode we want to process
        // styles for the dark mode. Because theme can change without reloading
        if mode == ColorMode::LightMode && !capabilities.supports_dark_mode_via_media_query {
            return Self::NoDarkMode;
        }

        match sender {
            Some(sender) if LIST_OF_TRUSTED_SENDERS.contains(&sender) => Self::Native,
            _ => Self::Injected,
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;
    use test_case::test_case;

    const SUPPORTS_MEDIA: BrowserCapabilities = BrowserCapabilities {
        supports_dark_mode_via_media_query: true,
    };
    const DOESNT_SUPPORT_MEDIA: BrowserCapabilities = BrowserCapabilities {
        supports_dark_mode_via_media_query: false,
    };

    #[test_case(None, ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Injected; "case 1")]
    #[test_case(None, ColorMode::LightMode, DOESNT_SUPPORT_MEDIA => DarkStyleSupportLevel::NoDarkMode ; "case 2")]
    #[test_case(Some("test@pm.me"), ColorMode::DarkMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Native ; "case 3")]
    #[test_case(Some("test@pm.me"), ColorMode::LightMode, SUPPORTS_MEDIA => DarkStyleSupportLevel::Native ; "case 4")]
    #[test_case(Some("test@pm.me"), ColorMode::LightMode, DOESNT_SUPPORT_MEDIA => DarkStyleSupportLevel::NoDarkMode ; "case 5")]
    fn test_support_level(
        sender: Option<&str>,
        mode: ColorMode,
        capabilities: BrowserCapabilities,
    ) -> DarkStyleSupportLevel {
        DarkStyleSupportLevel::new_for_html(sender, mode, capabilities)
    }
}
