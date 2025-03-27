use crate::{UniffiEnum, UniffiRecord};
use proton_core_common::models::{
    AppAppearance as RealAppAppearance, AppSettings as RealAppSettings,
    ProtectionAutoLock as RealAutoLock,
};

#[derive(Debug, UniffiRecord)]
pub struct AppSettings {
    pub appearance: Option<AppAppearance>,
    pub auto_lock: Option<AutoLock>,
    pub use_combine_contacts: Option<bool>,
    pub use_alternative_routing: Option<bool>,
}

impl AppSettings {
    #[must_use]
    pub fn merge_with_current(self, mut current: RealAppSettings) -> RealAppSettings {
        if let Some(appearance) = self.appearance {
            current.appearance = appearance.into();
        }

        if let Some(auto_lock) = self.auto_lock {
            current.auto_lock = auto_lock.into();
        }

        if let Some(use_combine_contacts) = self.use_combine_contacts {
            current.use_combine_contacts = use_combine_contacts;
        }

        if let Some(use_alternative_routing) = self.use_alternative_routing {
            current.use_alternative_routing = use_alternative_routing;
        }

        current
    }
}

#[derive(Debug, Copy, Clone, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum AppAppearance {
    System = 0,
    DarkMode = 1,
    LightMode = 2,
}

impl From<AppAppearance> for RealAppAppearance {
    fn from(value: AppAppearance) -> Self {
        match value {
            AppAppearance::System => Self::System,
            AppAppearance::DarkMode => Self::DarkMode,
            AppAppearance::LightMode => Self::LightMode,
        }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, UniffiEnum)]
pub enum AutoLock {
    Always,
    Minutes(u8),
}

impl From<AutoLock> for RealAutoLock {
    fn from(value: AutoLock) -> Self {
        match value {
            AutoLock::Always => Self::Always,
            AutoLock::Minutes(minutes) => Self::Minutes(minutes),
        }
    }
}
