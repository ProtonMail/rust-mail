use proton_api_mail::proton_api_core::exports::serde::Serialize;
use proton_api_mail::proton_api_core::exports::serde_json;
use proton_sqlite3::rusqlite::types::ToSqlOutput;
use proton_sqlite3::rusqlite::ToSql;
use stash::orm::ConversionError;
use stash::stash::StashError;
use std::ops::Deref;
use std::str;

pub struct JsonWriteBuffer {
    b: Vec<u8>,
}

impl Default for JsonWriteBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonWriteBuffer {
    pub fn new() -> Self {
        const DEFAULT_CAPACITY: usize = 64;
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            b: Vec::with_capacity(capacity),
        }
    }

    pub fn serialize<T: Serialize + ?Sized>(
        &mut self,
        v: &T,
    ) -> Result<JsonWriteBufferResult<'_>, StashError> {
        serde_json::to_writer(&mut self.b, v).map_err(|err| {
            StashError::DeserializationError(ConversionError::SerializationError(err.to_string()))
        })?;
        Ok(JsonWriteBufferResult { buffer: self })
    }

    pub fn clear(&mut self) {
        self.b.clear()
    }
}

pub struct JsonWriteBufferResult<'w> {
    buffer: &'w mut JsonWriteBuffer,
}

impl<'w> Drop for JsonWriteBufferResult<'w> {
    fn drop(&mut self) {
        self.buffer.clear()
    }
}

impl<'w> Deref for JsonWriteBufferResult<'w> {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl<'w> AsRef<str> for JsonWriteBufferResult<'w> {
    fn as_ref(&self) -> &str {
        // SAFETY: serde_json does never produce invalid utf8
        unsafe { str::from_utf8_unchecked(&self.buffer.b) }
    }
}

impl<'w> ToSql for JsonWriteBufferResult<'w> {
    fn to_sql(&self) -> proton_sqlite3::rusqlite::Result<ToSqlOutput<'_>> {
        self.as_ref().to_sql()
    }
}
