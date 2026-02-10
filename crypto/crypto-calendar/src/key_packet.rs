use base64::{Engine, prelude::BASE64_STANDARD};
use proton_calendar_api::CalendarEvent;

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
    pub fn as_ref(&self) -> KeyPacketRef<'_> {
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
    pub fn as_ref(&self) -> KeyPackets<KeyPacketRef<'_>> {
        KeyPackets {
            address_key_packet: self.address_key_packet.as_ref().map(KeyPacket::as_ref),
            shared_key_packet: self.shared_key_packet.as_ref().map(KeyPacket::as_ref),
        }
    }
}

impl<'a> KeyPackets<KeyPacketRef<'a>> {
    pub fn from_event(event: &'a CalendarEvent) -> Self {
        let address_key_packet = event
            .address_key_packet
            .as_deref()
            .map(KeyPacketRef::from_base64);

        let shared_key_packet = event
            .shared_key_packet
            .as_deref()
            .map(KeyPacketRef::from_base64);

        Self {
            address_key_packet,
            shared_key_packet,
        }
    }
}
