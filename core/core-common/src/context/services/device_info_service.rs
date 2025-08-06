use async_trait::async_trait;

use crate::{
    CoreContextError,
    device::{DeviceInfo, DynDeviceInfoProvider},
};

use super::Service;

pub struct DeviceInfoService {
    provider: Option<DynDeviceInfoProvider>,
}

#[async_trait]
impl Service for DeviceInfoService {
    type Error = CoreContextError;
}

impl DeviceInfoService {
    #[must_use]
    pub fn new(provider: Option<DynDeviceInfoProvider>) -> Self {
        Self { provider }
    }

    #[must_use]
    pub fn provider(&self) -> Option<&DynDeviceInfoProvider> {
        self.provider.as_ref()
    }

    pub async fn get_device_info(&self) -> Option<DeviceInfo> {
        let provider = self.provider.as_ref()?;
        Some(provider.get_device_info().await)
    }
}
