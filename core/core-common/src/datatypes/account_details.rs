use super::avatar::AvatarInformation;

/// Represents detailed information about a user account.
///
/// This struct contains the name, email, and avatar information
/// associated with an account.
#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct AccountDetails {
    /// The user's display name.
    pub name: String,
    /// The user's email address.
    pub email: String,
    /// Information about the user's avatar.
    pub avatar_information: AvatarInformation,
}
