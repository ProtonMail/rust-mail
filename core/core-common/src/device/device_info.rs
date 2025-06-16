use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use std::{ops::Deref, sync::Arc};

/// A dynamic device info provider.
pub type DynDeviceInfoProvider = Arc<dyn DeviceInfoProvider>;

/// The result of a device info request.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// The language code of this Locale.
    #[serde(rename = "appLang")]
    pub language: String,
    /// Time zone id, such as "Asia/Calcutta", "GMT+5:30" or "PST".
    pub timezone: String,
    /// Time zone raw offset in minutes from GMT including daylight saving.
    pub timezone_offset: i32,
    /// The end-user-visible name for the end product.
    #[serde(rename = "deviceName")]
    pub model: String,
    /// The consumer-visible brand with which the product/hardware will be associated.
    #[serde(rename = "deviceBrand")]
    pub brand: String,
    /// The name of the industrial design.
    #[serde(rename = "deviceCodename")]
    pub codename: String,
    /// The device's UUID.
    pub uuid: String,
    /// The country/region code, in ISO 3166 2-letter code, or a UN M.49 3-digit code.
    #[serde(rename = "regionCode")]
    pub country: String,
    /// If device/OS is rooted/jailbroken.
    #[serde(rename = "isJailbreak")]
    pub rooted: bool,
    /// The current scaling factor for fonts, relative to the base density scaling.
    #[serde(rename = "preferredContentSize")]
    pub font_scale: String,
    /// The total size of the device storage in GB.
    #[serde(rename = "storageCapacity")]
    pub storage: f64,
    /// If the device (or current context) is using dark mode.
    #[serde(rename = "isDarkmodeOn")]
    pub dark_mode: bool,
    /// List of enabled input methods application name (e.g. packageName, bundle id).
    pub keyboards: Vec<String>,
}

impl DeviceInfo {
    #[must_use]
    pub fn device_name(&self) -> String {
        format!("{}/{} {}", &self.model, &self.brand, &self.codename)
    }

    #[must_use]
    pub fn device_name_hash(&self) -> String {
        hex::encode(sha2::Sha256::digest(self.device_name()))
    }
}

/// An interface by which device info can be requested/provided.
#[async_trait]
pub trait DeviceInfoProvider: Send + Sync {
    async fn get_device_info(&self) -> DeviceInfo;
}

#[async_trait]
impl<T: ?Sized> DeviceInfoProvider for Arc<T>
where
    T: DeviceInfoProvider,
{
    async fn get_device_info(&self) -> DeviceInfo {
        self.deref().get_device_info().await
    }
}
