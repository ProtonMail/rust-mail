use crate::session::types::EncryptedUserSession;
use crate::{DBResult, SessionSqliteConnectionImpl};
use proton_api_core::domain::{Uid, UserId};
use proton_sqlite3::rusqlite::OptionalExtension;

impl<'c> SessionSqliteConnectionImpl<'c> {
    pub fn store_session(&mut self, session: &EncryptedUserSession) -> DBResult<()> {
        self.0.execute(
            "INSERT OR REPLACE INTO core_sessions VALUES (?,?,?,?,?,?,?,?)",
            (
                &session.session_id,
                &session.user_id,
                &session.email,
                &session.name,
                &session.access_token,
                &session.refresh_token,
                &session.scopes,
                &session.product,
            ),
        )?;
        Ok(())
    }

    pub fn load_session(&self, user_id: &UserId) -> DBResult<Option<EncryptedUserSession>> {
        let mut stmt = self
            .0
            .prepare("SELECT * FROM core_sessions WHERE user_id=?")?;
        stmt.query_row([user_id], |r| {
            Ok(EncryptedUserSession {
                session_id: r.get(0)?,
                user_id: r.get(1)?,
                email: r.get(2)?,
                name: r.get(3)?,
                access_token: r.get(4)?,
                refresh_token: r.get(5)?,
                scopes: r.get(6)?,
                product: r.get(7)?,
            })
        })
        .optional()
    }

    pub fn delete_session(&self, session_id: &Uid) -> DBResult<()> {
        self.0
            .execute("DELETE FROM core_sessions WHERE id =?", [session_id])?;
        Ok(())
    }

    pub fn delete_session_with_user_id(&self, user_id: &UserId) -> DBResult<()> {
        self.0
            .execute("DELETE FROM core_sessions WHERE id =?", [user_id])?;
        Ok(())
    }
}
