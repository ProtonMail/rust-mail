use super::MessageError;

pub fn to_sanitized_string(data: &[u8]) -> Result<String, MessageError> {
    let data_as_string = std::str::from_utf8(data)?;
    let sanitized_body = data_as_string.replace("\r\n", "\n");
    Ok(sanitized_body)
}
