use crate::db::session::types::EncryptedUserSession;
use crate::db::{
    DBResult, EncryptedAccessToken, EncryptedData, EncryptedRefreshToken,
    SessionSqliteConnectionImpl,
};
use proton_api_core::auth::Scope;
use proton_api_core::domain::{Uid, UserId};
use proton_sqlite3::rusqlite::{OptionalExtension, Row};
use proton_sqlite3::utils::mapped_rows_to_vec;

impl<'c> SessionSqliteConnectionImpl<'c> {
    /// Create or update a session.
    ///
    /// # Errors
    /// Returns error if the operation failed.
    pub fn create_or_update_session(&mut self, session: &EncryptedUserSession) -> DBResult<()> {
        self.0.execute(
            "INSERT OR REPLACE INTO core_sessions VALUES (?,?,?,?,?,?,?)",
            (
                &session.session_id,
                &session.user_id,
                &session.email,
                &session.name,
                session.access_token.as_ref(),
                &session.refresh_token.as_ref(),
                &session.scopes,
            ),
        )?;
        Ok(())
    }

    /// Update a session auth data after refresh.
    ///
    /// # Errors
    /// Returns error if the operation failed.
    pub fn update_session(
        &mut self,
        user_id: &UserId,
        session_id: &Uid,
        access_token: &EncryptedAccessToken,
        refresh_token: &EncryptedRefreshToken,
        scopes: &Scope,
    ) -> DBResult<()> {
        self.0.execute(
            "UPDATE core_sessions SET access_token=?, refresh_token=?, scopes=?,id=? WHERE user_id=?",
            (access_token.as_ref(), refresh_token.as_ref(), scopes, session_id, user_id),
        )?;
        Ok(())
    }

    /// Retrieve all stored sessions.
    ///
    /// # Errors
    /// Returns error if the operation failed.
    pub fn load_all_sessions(&self) -> DBResult<Vec<EncryptedUserSession>> {
        let mut stmt = self.0.prepare(EncryptedUserSessionSelector::query())?;
        let r = mapped_rows_to_vec(stmt.query_map((), EncryptedUserSessionSelector::from_row)?)?;
        Ok(r)
    }

    /// Get a session with the given session `id`.
    ///
    /// # Errors
    /// Returns error if the operation failed.
    pub fn get_session(&self, id: &Uid) -> DBResult<Option<EncryptedUserSession>> {
        let mut stmt = self
            .0
            .prepare(EncryptedUserSessionSelector::query_with_id())?;
        stmt.query_row([id], EncryptedUserSessionSelector::from_row)
            .optional()
    }

    /// Get a session with the given `user_id`.
    ///
    /// # Errors
    /// Returns error if the operation failed.
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

    /// Delete a session with the given `session_id`.
    ///
    /// # Errors
    /// Returns error if the operation failed.
    pub fn delete_session(&self, session_id: &Uid) -> DBResult<()> {
        self.0
            .execute("DELETE FROM core_sessions WHERE id =?", [session_id])?;
        Ok(())
    }

    /// Delete a session with the given `user_id`.
    ///
    /// # Errors
    /// Returns error if the operation failed.
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
            access_token: r.get::<usize, EncryptedData>(4).map(EncryptedAccessToken)?,
            refresh_token: r
                .get::<usize, EncryptedData>(5)
                .map(EncryptedRefreshToken)?,
            scopes: r.get(6)?,
        })
    }
}
