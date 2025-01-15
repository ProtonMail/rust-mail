use super::avatar::AvatarInformation;

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct AccountDetails {
    pub name: String,
    pub email: String,
    pub avatar_information: AvatarInformation
}
