use crate::{UniffiEnum, UniffiRecord};
use proton_core_common::models::{
    AppAppearance as RealAppAppearance, AppProtection as RealAppProtection,
    AppSettings as RealAppSettings, ProtectionAutoLock as RealAutoLock,
};

/// Struct Representing `AppSettings` - cross accounts settings of the application.
///
#[derive(Debug, UniffiRecord)]
pub struct AppSettings {
    /// The theme of the Application
    pub appearance: AppAppearance,

    /// What additional protection of the app is in use.
    pub protection: AppProtection,

    /// Autolock time for additional protection to kick in,
    /// when app is running in bg for extended time.
    pub auto_lock: AutoLock,

    /// Do you want to share contacts between the accounts.
    pub use_combine_contacts: bool,

    /// Use alternative routing, helpful for ppl leaving in
    /// area where Proton servers are blocked for any reason.
    pub use_alternative_routing: bool,
}

impl From<RealAppSettings> for AppSettings {
    fn from(value: RealAppSettings) -> Self {
        Self {
            appearance: value.appearance.into(),
            protection: value.protection.into(),
            auto_lock: value.auto_lock.into(),
            use_combine_contacts: value.use_combine_contacts,
            use_alternative_routing: value.use_alternative_routing,
        }
    }
}

/// Representation of diff of selected setting options
/// and stored local value of `AppSettings`
///
/// If value was modified by the user, client suppose to include this value
/// as Some(value) in this `Record`.
///
/// If value is suppose to left unchananged, client should left the field as `None`
///
#[derive(Debug, UniffiRecord)]
pub struct AppSettingsDiff {
    /// The theme of the Application
    pub appearance: Option<AppAppearance>,

    /// Autolock time for additional protection to kick in,
    /// when app is running in bg for extended time.
    pub auto_lock: Option<AutoLock>,

    /// Do you want to share contacts between the accounts.
    pub use_combine_contacts: Option<bool>,

    /// Use alternative routing, helpful for ppl leaving in
    /// area where Proton servers are blocked for any reason.
    pub use_alternative_routing: Option<bool>,
}

impl AppSettingsDiff {
    /// Merge set `Some(value)` values from Record in current AppSettings database entry
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

/// Representation of available themes for the app.
///
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

impl From<RealAppAppearance> for AppAppearance {
    fn from(value: RealAppAppearance) -> Self {
        match value {
            RealAppAppearance::System => Self::System,
            RealAppAppearance::DarkMode => Self::DarkMode,
            RealAppAppearance::LightMode => Self::LightMode,
        }
    }
}

/// Supported additional protection for accessing app.
///
#[derive(Debug, Copy, Clone, UniffiEnum)]
#[repr(u8)]
pub enum AppProtection {
    None = 0,
    Biometrics = 1,
    Pin = 2,
}

impl From<RealAppProtection> for AppProtection {
    fn from(value: RealAppProtection) -> Self {
        match value {
            RealAppProtection::None => Self::None,
            RealAppProtection::Biometrics => Self::Biometrics,
            RealAppProtection::Pin => Self::Pin,
        }
    }
}

/// How much time till app in the background will require
/// authentication when going to foreground.
///
#[derive(Debug, Copy, Clone, PartialEq, UniffiEnum)]
pub enum AutoLock {
    Always,
    Minutes(u8),
    Never,
}

impl From<AutoLock> for RealAutoLock {
    fn from(value: AutoLock) -> Self {
        match value {
            AutoLock::Always => Self::Always,
            AutoLock::Minutes(minutes) => Self::Minutes(minutes),
            AutoLock::Never => Self::Never,
        }
    }
}

impl From<RealAutoLock> for AutoLock {
    fn from(value: RealAutoLock) -> Self {
        match value {
            RealAutoLock::Always => Self::Always,
            RealAutoLock::Minutes(minutes) => Self::Minutes(minutes),
            RealAutoLock::Never => Self::Never,
        }
    }
}
