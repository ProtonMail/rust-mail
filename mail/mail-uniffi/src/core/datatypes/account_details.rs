use crate::core::datatypes::AvatarInformation;
use crate::UniffiRecord;
use proton_core_common::datatypes::AccountDetails as RealAccountDetails;

#[derive(Clone, Debug, Eq, PartialEq, UniffiRecord)]
pub struct AccountDetails {
    pub name: String,
    pub email: String,
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
