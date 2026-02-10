//! Module grouping all value specific logic

pub mod component;
pub mod date;
pub mod date_and_or_time;
pub mod date_time;
pub mod iana_token;
pub mod list_component;
pub mod param_value;
pub mod text;
pub mod text_list;
pub mod time;
pub mod timestamp;
pub mod uri;
pub mod utc_offset;
pub mod x_name;
pub mod zone;

// Check that comma separated substring in value respect the given predicate
pub(crate) fn check_list(
    value: &str,
    predicate: impl Fn(&str) -> bool,
    separator: char,
) -> Option<u32> {
    let mut offset = 0;
    let mut start = 0;
    let mut count = 0;
    while let Some(position) = value[offset..].find(separator) {
        offset += position + 1;
        if offset < 2 {
            // value start with separator
            start = offset;
            count += 1;
        } else if value.get(offset - 2..offset - 1) != Some(r"\") {
            if !predicate(&value[start..offset - 1]) {
                return None;
            }
            start = offset;
            count += 1;
        }
    }
    if predicate(&value[start..]) {
        Some(count + 1)
    } else {
        None
    }
}
