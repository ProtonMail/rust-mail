/// What capabilities browser has. Based on that information we use different
/// strategies for transforming the HTML content
#[derive(Clone, Copy, Debug)]
pub struct BrowserCapabilities {
    pub supports_dark_mode_via_media_query: bool,
}
