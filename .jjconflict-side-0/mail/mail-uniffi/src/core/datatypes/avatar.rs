use crate::UniffiRecord;
use proton_core_common::datatypes::AvatarInformation as RealAvatarInformation;

/// TODO: Document this struct.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct AvatarInformation {
    /// TODO: Document this field.
    pub text: String,

    /// TODO: Document this field.
    pub color: String,
}

impl From<AvatarInformation> for RealAvatarInformation {
    fn from(value: AvatarInformation) -> Self {
        RealAvatarInformation {
            text: value.text,
            color: value.color,
        }
    }
}

impl From<RealAvatarInformation> for AvatarInformation {
    fn from(value: RealAvatarInformation) -> Self {
        AvatarInformation {
            text: value.text,
            color: value.color,
        }
    }
}
