use crate::UniffiEnum;
use proton_core_common::models::AppProtection as RealAppProtection;

#[derive(Debug, Copy, Clone, PartialEq, UniffiEnum)]
#[repr(u8)]
pub enum AppProtection {
    None = 0,
    Biometrics = 1,
    Pin = 2,
}

impl From<RealAppProtection> for AppProtection {
    fn from(value: RealAppProtection) -> Self {
        match value {
            RealAppProtection::None => AppProtection::None,
            RealAppProtection::Biometrics => AppProtection::Biometrics,
            RealAppProtection::Pin => AppProtection::Pin,
        }
    }
}
