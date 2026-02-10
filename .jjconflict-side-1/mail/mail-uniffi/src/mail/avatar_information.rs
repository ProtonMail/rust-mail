use crate::core::datatypes::AvatarInformation;
use crate::mail::datatypes::{MessageRecipient, MessageSender};
use proton_core_common::datatypes::AvatarInformation as RealAvatarInformation;
use proton_core_common::utils::MapVec;
use proton_mail_common::datatypes::MessageRecipient as RealMessageRecipient;
use proton_mail_common::datatypes::MessageSender as RealMessageSender;

#[uniffi_export]
pub fn avatar_information_from_message_senders(
    address_list: Vec<MessageSender>,
) -> AvatarInformation {
    RealMessageSender::avatar_info(&address_list.map_vec()).into()
}

#[uniffi_export]
pub fn avatar_information_from_message_sender(address: MessageSender) -> AvatarInformation {
    RealAvatarInformation::from(RealMessageSender::from(address)).into()
}

#[uniffi_export]
pub fn avatar_information_from_message_recipients(
    address_list: Vec<MessageRecipient>,
) -> AvatarInformation {
    RealMessageRecipient::avatar_info(&address_list.map_vec()).into()
}

#[uniffi_export]
pub fn avatar_information_from_message_recipient(address: MessageRecipient) -> AvatarInformation {
    RealAvatarInformation::from(RealMessageRecipient::from(address)).into()
}
