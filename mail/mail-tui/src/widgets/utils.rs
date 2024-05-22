use chrono::DateTime;
use proton_api_mail::domain::MessageAddress;

pub fn date_from_timestamp(timestamp: u64) -> String {
    let timestamp_i64 = i64::try_from(timestamp).unwrap_or(0);
    let date = DateTime::<chrono::Utc>::from_timestamp(timestamp_i64, 0).unwrap();
    let date = DateTime::<chrono::Local>::from(date);
    let date_str = date.format("%d/%m/%Y %H:%M");
    date_str.to_string()
}
pub fn sender_name(s: &MessageAddress) -> &str {
    if s.name.is_empty() {
        s.address.as_str()
    } else {
        s.name.as_str()
    }
}
