use base64::{prelude::BASE64_STANDARD, Engine};

#[derive(Clone, Debug)]
pub struct KeyPacket(String);

impl KeyPacket {
    #[must_use]
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self::from_base64(BASE64_STANDARD.encode(bytes))
    }

    #[must_use]
    pub fn from_base64(packet: String) -> Self {
        Self(packet)
    }

    #[must_use]
    pub fn as_base64(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn into_base64(self) -> String {
        self.0
    }

    #[must_use]
    pub fn as_ref(&self) -> KeyPacketRef {
        KeyPacketRef::from_base64(self.as_base64())
    }
}

#[derive(Clone, Copy, Debug)]
pub struct KeyPacketRef<'a>(&'a str);

impl<'a> KeyPacketRef<'a> {
    #[must_use]
    pub fn from_base64(key: &'a str) -> Self {
        Self(key)
    }

    #[must_use]
    pub fn as_base64(&self) -> &'a str {
        self.0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct KeyPackets<T> {
    pub address_key_packet: Option<T>,
    pub shared_key_packet: Option<T>,
}

impl KeyPackets<KeyPacket> {
    pub fn as_ref(&self) -> KeyPackets<KeyPacketRef> {
        KeyPackets {
            address_key_packet: self.address_key_packet.as_ref().map(KeyPacket::as_ref),
            shared_key_packet: self.shared_key_packet.as_ref().map(KeyPacket::as_ref),
        }
    }
}
