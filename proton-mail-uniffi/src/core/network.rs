/// Callback when the network status in the context has changed.
#[uniffi::export(callback_interface)]
pub trait NetworkStatusChanged: Send + Sync {
    fn on_network_status_changed(&self, online: bool);
}

pub(crate) struct FFINetworkStatusChanged(Box<dyn NetworkStatusChanged>);
impl From<Box<dyn NetworkStatusChanged>> for FFINetworkStatusChanged {
    fn from(value: Box<dyn NetworkStatusChanged>) -> Self {
        Self(value)
    }
}

impl proton_core_common::NetworkStatusChanged for FFINetworkStatusChanged {
    fn on_network_status_changed(&self, online: bool) {
        self.0.on_network_status_changed(online);
    }
}
