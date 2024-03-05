#[cfg(test)]
mod tests;

use crate::{DBResult, MailSqliteConnectionImpl};
use proton_api_mail::proton_api_core::domain::EventId;
use proton_sqlite3::rusqlite::OptionalExtension;

impl<'c> MailSqliteConnectionImpl<'c> {
    pub fn set_last_event_id(&mut self, type_id: &str, event_id: &EventId) -> DBResult<()> {
        self.0.execute(
            "INSERT OR REPLACE INTO event_id_store VALUES (?,?)",
            (type_id, event_id),
        )?;
        Ok(())
    }

    pub fn get_last_event_id(&self, type_id: &str) -> DBResult<Option<EventId>> {
        self.0
            .query_row(
                "SELECT value FROm event_id_store WHERE id=?",
                [type_id],
                |r| r.get(0),
            )
            .optional()
    }

    pub fn delete_last_event_id(&self, type_id: &str) -> DBResult<()> {
        self.0
            .execute("DELETE FROM event_id_store WHERE id=?", [type_id])?;
        Ok(())
    }
}
