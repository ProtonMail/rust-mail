use proton_mail_common::{avatar::AvatarInformation, proton_api_mail::domain::MessageAddress};

/// Creates an [`AvatarInformation`] by taking then display name and email address
/// and uses these to determine the text and color the avatar should be.
#[uniffi::export]
pub fn avatar_information_from_name_and_email(
    display_name: &str,
    email: &str,
) -> AvatarInformation {
    AvatarInformation::build(display_name, email)
}

/// Creates an [`AvatarInformation`] struct using the details of the first [`MessageAddress`] in the provided slice.
#[uniffi::export]
pub fn avatar_information_from_message_addresses(
    address_list: &[MessageAddress],
) -> AvatarInformation {
    AvatarInformation::from_message_addresses(address_list)
}

/// Creates an [`AvatarInformation`] struct using a [`MessageAddress`].
#[uniffi::export]
pub fn avatar_information_from_message_address(address: &MessageAddress) -> AvatarInformation {
    AvatarInformation::from_message_address(address)
}
