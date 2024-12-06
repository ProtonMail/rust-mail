use crate::core::datatypes::AvatarInformation;
use crate::mail::datatypes::{MessageRecipient, MessageSender};
use proton_core_common::datatypes::AvatarInformation as RealAvatarInformation;
use proton_mail_common::datatypes::MessageRecipient as RealMessageRecipient;
use proton_mail_common::datatypes::MessageSender as RealMessageSender;

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
pub fn avatar_information_from_message_senders(
    address_list: &[MessageSender],
) -> AvatarInformation {
    let addresses: Vec<RealMessageSender> = address_list
        .iter()
        .map(|addr| RealMessageSender::from(addr.clone()))
        .collect();
    RealMessageSender::avatar_info(&addresses).into()
}

/// Creates an [`AvatarInformation`] struct using a [`MessageAddress`].
#[uniffi::export]
pub fn avatar_information_from_message_sender(address: &MessageSender) -> AvatarInformation {
    RealAvatarInformation::from(RealMessageSender::from(address.clone())).into()
}

/// Creates an [`AvatarInformation`] struct using the details of the first [`MessageAddress`] in the provided slice.
#[uniffi::export]
pub fn avatar_information_from_message_recipients(
    address_list: &[MessageRecipient],
) -> AvatarInformation {
    let addresses: Vec<RealMessageRecipient> = address_list
        .iter()
        .map(|addr| RealMessageRecipient::from(addr.clone()))
        .collect();
    RealMessageRecipient::avatar_info(&addresses).into()
}

/// Creates an [`AvatarInformation`] struct using a [`MessageAddress`].
#[uniffi::export]
pub fn avatar_information_from_message_recipient(address: &MessageRecipient) -> AvatarInformation {
    RealAvatarInformation::from(RealMessageRecipient::from(address.clone())).into()
}
