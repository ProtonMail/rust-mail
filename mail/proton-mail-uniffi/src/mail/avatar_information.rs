use crate::mail::datatypes::{AvatarInformation, MessageAddress};
use proton_mail_common::avatar::AvatarInformation as RealAvatarInformation;
use proton_mail_common::datatypes::MessageAddress as RealMessageAddress;

/// Creates an [`AvatarInformation`] by taking then display name and email address
/// and uses these to determine the text and color the avatar should be.
#[uniffi::export]
pub fn avatar_information_from_name_and_email(
    display_name: &str,
    email: &str,
) -> AvatarInformation {
    RealAvatarInformation::build(display_name, email).into()
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
    RealAvatarInformation::from_message_addresses(&addresses).into()
}

/// Creates an [`AvatarInformation`] struct using a [`MessageAddress`].
#[uniffi::export]
pub fn avatar_information_from_message_address(address: &MessageAddress) -> AvatarInformation {
    RealAvatarInformation::from_message_address(&address.clone().into()).into()
}
