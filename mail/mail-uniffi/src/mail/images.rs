use proton_mail_common::ImagePolicy as RealImagePolicy;
use uniffi::Enum;

#[derive(Clone, Copy, Debug, Enum)]
pub enum ImagePolicy {
    /// Swap image's protocol from `http` to `https` and allow the image to be
    /// proxied through Proton severs (assuming user has this option enabled).
    Safe,

    /// Load image as-is, without changing the protocol and without passing it
    /// through the proxy.
    Unsafe,
}

impl From<ImagePolicy> for RealImagePolicy {
    fn from(value: ImagePolicy) -> Self {
        match value {
            ImagePolicy::Safe => RealImagePolicy::Safe,
            ImagePolicy::Unsafe => RealImagePolicy::Unsafe,
        }
    }
}
