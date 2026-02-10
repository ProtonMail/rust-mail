use super::MessageError;

/// `GettablePGPMessage` is a trait for unifying how structs return encrypted PGP messages for use
/// in operations like decryption or separating keys and data packets
pub trait GettablePGPMessage {
    /// Return a byte slice of a PGP message
    fn pgp_message(&self) -> &[u8];
}

pub fn to_sanitized_string(data: &[u8]) -> Result<String, MessageError> {
    let data_as_string = std::str::from_utf8(data)?;
    let sanitized_body = data_as_string.replace("\r\n", "\n");
    Ok(sanitized_body)
}
