use crate::session::types::EncryptedUserSession;
use crate::{DBResult, EncryptedData, SessionSqliteConnectionImpl};
use proton_api_core::auth::AuthScope;
use proton_api_core::domain::{Uid, UserId};
use proton_sqlite3::rusqlite::{OptionalExtension, Row};
use proton_sqlite3::utils::mapped_rows_to_vec;

impl<'c> SessionSqliteConnectionImpl<'c> {
    pub fn create_or_update_session(&mut self, session: &EncryptedUserSession) -> DBResult<()> {
        self.0.execute(
            "INSERT OR REPLACE INTO core_sessions VALUES (?,?,?,?,?,?,?)",
            (
                &session.session_id,
                &session.user_id,
                &session.email,
                &session.name,
                &session.access_token,
                &session.refresh_token,
                &session.scopes,
            ),
        )?;
        Ok(())
    }

    pub fn update_session(
        &mut self,
        user_id: &UserId,
        session_id: &Uid,
        access_token: &EncryptedData,
        refresh_token: &EncryptedData,
        scopes: &AuthScope,
    ) -> DBResult<()> {
        self.0.execute(
            "UPDATE core_sessions SET access_token=?, refresh_token=?, scopes=?,id=? WHERE user_id=?",
            (access_token, refresh_token, scopes, session_id, user_id),
        )?;
        Ok(())
    }

    pub fn load_all_sessions(&self) -> DBResult<Vec<EncryptedUserSession>> {
        let mut stmt = self.0.prepare(EncryptedUserSessionSelector::query())?;
        let r = mapped_rows_to_vec(stmt.query_map((), EncryptedUserSessionSelector::from_row)?)?;
        Ok(r)
    }

    pub fn get_session(&self, id: &Uid) -> DBResult<Option<EncryptedUserSession>> {
        let mut stmt = self
            .0
            .prepare(EncryptedUserSessionSelector::query_with_id())?;
        stmt.query_row([id], EncryptedUserSessionSelector::from_row)
            .optional()
    }

    pub fn get_session_with_user_id(
        &self,
        user_id: &UserId,
    ) -> DBResult<Option<EncryptedUserSession>> {
        let mut stmt = self
            .0
            .prepare(EncryptedUserSessionSelector::query_with_user_id())?;
        stmt.query_row([user_id], EncryptedUserSessionSelector::from_row)
            .optional()
    }

    pub fn delete_session(&self, session_id: &Uid) -> DBResult<()> {
        self.0
            .execute("DELETE FROM core_sessions WHERE id =?", [session_id])?;
        Ok(())
    }

    pub fn delete_session_with_user_id(&self, user_id: &UserId) -> DBResult<()> {
        self.0
            .execute("DELETE FROM core_sessions WHERE user_id =?", [user_id])?;
        Ok(())
    }
}

struct EncryptedUserSessionSelector {}

impl EncryptedUserSessionSelector {
    const fn query() -> &'static str {
        "SELECT * FROM core_sessions"
    }
    const fn query_with_id() -> &'static str {
        "SELECT * FROM core_sessions WHERE id=?"
    }

    const fn query_with_user_id() -> &'static str {
        "SELECT * FROM core_sessions WHERE user_id=?"
    }

    fn from_row(r: &Row) -> DBResult<EncryptedUserSession> {
        Ok(EncryptedUserSession {
            session_id: r.get(0)?,
            user_id: r.get(1)?,
            email: r.get(2)?,
            name: r.get(3)?,
            access_token: r.get(4)?,
            refresh_token: r.get(5)?,
            scopes: r.get(6)?,
        })
    }
}
