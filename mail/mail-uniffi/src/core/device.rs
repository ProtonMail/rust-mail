use futures::FutureExt;
use proton_core_common::device as device_info;
use std::{ops::Deref, sync::Arc};

pub type DynDeviceInfoProvider = Arc<dyn DeviceInfoProvider>;

/// The response to a device info request.
#[derive(Debug, uniffi::Record)]
#[allow(clippy::large_enum_variant)]
pub struct DeviceInfo {
    /// The language code of this Locale.
    language: String,
    /// Time zone id, such as "Asia/Calcutta", "GMT+5:30" or "PST".
    timezone: String,
    /// Time zone raw offset in minutes from GMT including daylight saving.
    timezone_offset: i32,
    /// The end-user-visible name for the end product.
    model: String,
    /// The consumer-visible brand with which the product/hardware will be associated.
    brand: String,
    /// The name of the industrial design.
    codename: String,
    /// The device's UUID.
    uuid: String,
    /// The country/region code, in ISO 3166 2-letter code, or a UN M.49 3-digit code.
    country: String,
    /// If device/OS is rooted/jailbroken.
    rooted: bool,
    /// The current scaling factor for fonts, relative to the base density scaling.
    font_scale: String,
    /// The total size of the device storage in GB.
    storage: f64,
    /// If the device (or current context) is using dark mode.
    dark_mode: bool,
    /// List of enabled input methods application name (e.g. packageName, bundle id).
    keyboards: Vec<String>,
}
impl From<DeviceInfo> for device_info::DeviceInfo {
    fn from(response: DeviceInfo) -> Self {
        Self {
            language: response.language,
            timezone: response.timezone,
            timezone_offset: response.timezone_offset,
            model: response.model,
            brand: response.brand,
            codename: response.codename,
            uuid: response.uuid,
            country: response.country,
            rooted: response.rooted,
            font_scale: response.font_scale,
            storage: response.storage,
            dark_mode: response.dark_mode,
            keyboards: response.keyboards,
        }
    }
}

/// An interface to provide device info from native.
#[uniffi::export(with_foreign)]
#[async_trait::async_trait]
pub trait DeviceInfoProvider: Send + Sync + 'static {
    async fn get_device_info(&self) -> DeviceInfo;
}

#[async_trait::async_trait]
impl<T: ?Sized + DeviceInfoProvider> DeviceInfoProvider for Arc<T> {
    async fn get_device_info(&self) -> DeviceInfo {
        self.deref().get_device_info().await
    }
}

/// Wraps a `DeviceInfoProvider` to implement the core `DeviceInfoProvider` trait.
pub(crate) struct DeviceInfoProviderWrap<T> {
    inner: T,
}

impl<T: DeviceInfoProvider> DeviceInfoProviderWrap<T> {
    /// Wrap a `DeviceInfoProvider` to implement the core `DeviceInfoProvider` trait.
    pub fn wrap(inner: T) -> device_info::DynDeviceInfoProvider {
        Arc::new(Self { inner })
    }
}

#[async_trait::async_trait]
impl<T: DeviceInfoProvider> device_info::DeviceInfoProvider for DeviceInfoProviderWrap<T> {
    async fn get_device_info(&self) -> device_info::DeviceInfo {
        self.inner.get_device_info().map_into().await
    }
}
