use crate::UniffiRecord;
use crate::core::datatypes::AvatarInformation;
use proton_core_common::datatypes::AccountDetails as RealAccountDetails;

/// Represents detailed information about a user account.
///
/// This struct contains the name, email, and avatar information
/// associated with an account.
#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct AccountDetails {
    /// The user's display name.
    pub name: String,
    /// The user's email address.
    pub email: String,
    /// Information about the user's avatar.
    pub avatar_information: AvatarInformation,
}

impl From<AccountDetails> for RealAccountDetails {
    fn from(account: AccountDetails) -> Self {
        RealAccountDetails {
            name: account.name,
            email: account.email,
            avatar_information: account.avatar_information.into(),
        }
    }
}

impl From<RealAccountDetails> for AccountDetails {
    fn from(account: RealAccountDetails) -> Self {
        AccountDetails {
            name: account.name,
            email: account.email,
            avatar_information: account.avatar_information.into(),
        }
    }
}
