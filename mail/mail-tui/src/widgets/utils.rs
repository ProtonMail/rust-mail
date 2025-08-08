use anyhow::{Context, anyhow};
use chrono::{DateTime, Local, MappedLocalTime, NaiveDateTime, TimeZone};
use crossterm::event::KeyCode;
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
        sender.name.as_clear_text_str()
    }
}

pub fn format_sender(sender: &MessageSender) -> String {
    if sender.name.is_empty() {
        sender.address.clone().into_clear_text_string()
    } else {
        format!(
            "{} <{}>",
            sender.name.as_clear_text_str(),
            sender.address.as_clear_text_str()
        )
    }
}

pub fn format_recipient(sender: &MessageRecipient) -> String {
    if sender.name.is_empty() {
        sender.address.clone().into_clear_text_string()
    } else {
        format!(
            "{} <{}>",
            sender.name.as_clear_text_str(),
            sender.address.as_clear_text_str()
        )
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

pub fn format_flags(starred: bool, rsvp: bool, expiration_time: UnixTimestamp) -> String {
    iter::once("")
        .chain(starred.then_some("*"))
        .chain(rsvp.then_some("R"))
        .chain((expiration_time.as_u64() != 0).then_some("E"))
        .collect()
}

/// Parse a date time string in the format `DD/MM/YYYY HH:MM` into local time.
pub fn parse_date_time(dt: &str) -> anyhow::Result<DateTime<Local>> {
    let dt =
        NaiveDateTime::parse_from_str(dt, "%d/%m/%Y %H:%M").context("Failed to parse date time")?;

    match Local.from_local_datetime(&dt) {
        MappedLocalTime::Single(dt) | MappedLocalTime::Ambiguous(dt, _) => Ok(dt),
        MappedLocalTime::None => Err(anyhow!("No local time found")),
    }
}

pub trait ScrollableState {
    fn next(&mut self);
    fn prev(&mut self);
    fn handle_event(&mut self, key: KeyCode) -> bool {
        match key {
            KeyCode::Char('k') | KeyCode::Up => {
                self.prev();
                true
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.next();
                true
            }
            _ => false,
        }
    }
}
