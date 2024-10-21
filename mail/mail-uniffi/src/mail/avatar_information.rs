use crate::core::datatypes::AvatarInformation;
use crate::mail::datatypes::MessageAddress;
use proton_core_common::datatypes::AvatarInformation as RealAvatarInformation;
use proton_mail_common::datatypes::MessageAddress as RealMessageAddress;

/// Creates an [`AvatarInformation`] by taking then display name and email address
/// and uses these to determine the text and color the avatar should be.
#[uniffi::export]
pub fn avatar_information_from_name_and_email(
    display_name: &str,
    email: &str,
) -> AvatarInformation {
    RealAvatarInformation::from(display_name)
        .or_else(email)
        .into()
}

/// Creates an [`AvatarInformation`] struct using the details of the first [`MessageAddress`] in the provided slice.
#[uniffi::export]
pub fn avatar_information_from_message_addresses(
    address_list: &[MessageAddress],
) -> AvatarInformation {
    let addresses: Vec<RealMessageAddress> = address_list
        .iter()
        .map(|addr| RealMessageAddress::from(addr.clone()))
        .collect();
    RealMessageAddress::avatar_info(&addresses).into()
}

/// Creates an [`AvatarInformation`] struct using a [`MessageAddress`].
#[uniffi::export]
pub fn avatar_information_from_message_address(address: &MessageAddress) -> AvatarInformation {
    RealAvatarInformation::from(RealMessageAddress::from(address.clone())).into()
}
