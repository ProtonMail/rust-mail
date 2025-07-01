use proton_core_common::datatypes::UnixTimestamp;
use proton_mail_common::datatypes::{
    MessageRecipient, MessageRecipients, MessageSender, MessageSenders,
};
use std::iter;

pub fn date_from_timestamp(timestamp: UnixTimestamp) -> String {
    let date = timestamp.to_date_time().unwrap_or_default();
    let date_str = date.format("%d/%m/%Y %H:%M");
    date_str.to_string()
}

pub fn sender_name(sender: &MessageSender) -> &str {
    if sender.name.is_empty() {
        sender.address.as_clear_text_str()
    } else {
        sender.name.as_str()
    }
}

pub fn format_sender(sender: &MessageSender) -> String {
    if sender.name.is_empty() {
        sender.address.clone().into_clear_text_string()
    } else {
        format!("{} <{}>", sender.name, sender.address.as_clear_text_str())
    }
}

pub fn format_recipient(sender: &MessageRecipient) -> String {
    if sender.name.is_empty() {
        sender.address.clone().into_clear_text_string()
    } else {
        format!("{} <{}>", sender.name, sender.address.as_clear_text_str())
    }
}

pub fn format_senders(senders: &MessageSenders) -> String {
    senders
        .value
        .iter()
        .map(format_sender)
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn format_recipients(senders: &MessageRecipients) -> String {
    senders
        .value
        .iter()
        .map(format_recipient)
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn format_flags(starred: bool, rsvp: bool) -> String {
    iter::once("")
        .chain(starred.then_some("*"))
        .chain(rsvp.then_some("R"))
        .collect()
}
