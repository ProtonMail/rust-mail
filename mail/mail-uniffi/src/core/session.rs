use std::{borrow::Borrow, sync::Arc};

use crate::core::datatypes::AccountDetails;
use proton_core_api::services::proton::SessionId;
use proton_core_common::{CoreAccountState, CoreSessionState};
use proton_core_common::{
    datatypes::{PasswordMode, TfaStatus},
    db::account::{CoreAccount, CoreSession},
};
use uniffi::{Enum, Record};

/// Represents an account known to the system.
#[derive(uniffi::Object)]
pub struct StoredAccount {
    account: CoreAccount,
    state: CoreAccountState,
}

impl StoredAccount {
    pub(crate) fn new(account: CoreAccount, state: CoreAccountState) -> Arc<Self> {
        Arc::new(Self { account, state })
    }

    pub(crate) fn account(&self) -> &CoreAccount {
        &self.account
    }
}

#[uniffi_export]
impl StoredAccount {
    /// Get the account's user id.
    #[must_use]
    pub fn user_id(&self) -> String {
        self.account.remote_id.to_string()
    }

    /// Retrieves account details including the name, email, and avatar information.
    ///
    /// This method constructs the account details using the available fields. If the display name
    /// or username is not set, it falls back to `name_or_addr`. Similarly, the email defaults to
    /// `name_or_addr` if the primary address is unavailable.
    ///
    /// # Returns
    /// - `AccountDetails`: A struct containing the account's name, email, and avatar information.
    #[must_use]
    pub fn details(&self) -> AccountDetails {
        self.account.details().into()
    }

    /// Returns whether the account has 2FA enabled.
    #[must_use]
    pub fn second_factor_status(&self) -> Option<SecondFactorStatus> {
        self.account.second_factor_mode.map(Into::into)
    }

    /// Returns whether the account has a second (mailbox) password.
    #[must_use]
    pub fn second_password_status(&self) -> Option<SecondPasswordStatus> {
        self.account.password_mode.map(Into::into)
    }

    /// Sequence number of when the account was last set as the primary account.
    #[must_use]
    pub fn primary_seq(&self) -> i64 {
        self.account.primary_seq
    }

    /// Get the state of the account.
    #[must_use]
    pub fn state(&self) -> StoredAccountState {
        self.state.borrow().into()
    }
}

/// Represents an account known to the system.
#[derive(uniffi::Object)]
pub struct StoredSession {
    session: CoreSession,
    state: CoreSessionState,
}

impl StoredSession {
    pub(crate) fn new(session: CoreSession, state: CoreSessionState) -> Arc<Self> {
        Arc::new(Self { session, state })
    }

    pub(crate) fn session(&self) -> &CoreSession {
        &self.session
    }
}

#[uniffi_export]
impl StoredSession {
    /// Get the ID of the session.
    #[must_use]
    pub fn session_id(&self) -> String {
        self.session.remote_id.to_string()
    }

    /// Get the account id of the session.
    #[must_use]
    pub fn user_id(&self) -> String {
        self.session.account_id.to_string()
    }

    /// Get the state of the session.
    #[must_use]
    pub fn state(&self) -> StoredSessionState {
        self.state.borrow().into()
    }
}

/// Represents the state of an account.
#[derive(Debug, Enum)]
pub enum StoredAccountState {
    /// The account is not yet ready to be used.
    NotReady,

    /// The account has at least one fully logged-in session;
    /// the variant holds the (remote) IDs of the fullly logged-in sessions.
    LoggedIn(Vec<String>),

    /// The account has authenticated sessions but they are missing the key secret.
    /// The variant holds the (remote) IDs of the sessions that are missing the key secret.
    NeedMbp(Vec<String>),

    /// The account has partially authenticated sessions that require a second factor.
    /// The variant holds the (remote) IDs of the sessions that require a second factor.
    NeedTfa(Vec<String>),

    /// The account has a temporary password that must be set before it can be used.
    /// The variant holds the (remote) IDs of the sessions that require a new password.
    NeedNewPass(Vec<String>),

    /// The account has no active sessions.
    LoggedOut,
}

impl From<CoreAccountState> for StoredAccountState {
    fn from(value: CoreAccountState) -> Self {
        Self::from(&value)
    }
}

impl From<&CoreAccountState> for StoredAccountState {
    fn from(value: &CoreAccountState) -> Self {
        fn from_inner(value: &[SessionId]) -> Vec<String> {
            value.iter().cloned().map(SessionId::into_inner).collect()
        }

        match value {
            CoreAccountState::NotReady => Self::NotReady,
            CoreAccountState::LoggedIn(vec) => Self::LoggedIn(from_inner(vec)),
            CoreAccountState::NeedMbp(vec) => Self::NeedMbp(from_inner(vec)),
            CoreAccountState::NeedTfa(vec) => Self::NeedTfa(from_inner(vec)),
            CoreAccountState::NeedNewPass(vec) => Self::NeedNewPass(from_inner(vec)),
            CoreAccountState::LoggedOut => Self::LoggedOut,
        }
    }
}

/// Represents the state of a session.
#[derive(Debug, Enum)]
pub enum StoredSessionState {
    /// The session is fully authenticated and ready to use.
    Authenticated,

    /// The session has authenticated but is missing the key secret.
    NeedKey,

    /// The session has partially authenticated and requires a second factor.
    NeedTfa,
}

impl From<CoreSessionState> for StoredSessionState {
    fn from(value: CoreSessionState) -> Self {
        Self::from(&value)
    }
}

impl From<&CoreSessionState> for StoredSessionState {
    fn from(value: &CoreSessionState) -> Self {
        match value {
            CoreSessionState::Authenticated => Self::Authenticated,
            CoreSessionState::NeedKey => Self::NeedKey,
            CoreSessionState::NeedTfa => Self::NeedTfa,
        }
    }
}

/// Represents the second factor status of an account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Record)]
pub struct SecondFactorStatus {
    /// Whether a TOTP second factor can be used.
    pub totp: bool,

    /// Whether a FIDO2 second factor can be used.
    pub fido: bool,
}

impl From<TfaStatus> for SecondFactorStatus {
    fn from(tfa: TfaStatus) -> Self {
        let (totp, fido) = match tfa {
            TfaStatus::None => (false, false),
            TfaStatus::Totp => (true, false),
            TfaStatus::Fido2 => (false, true),
            TfaStatus::TotpOrFido2 => (true, true),
        };

        Self { totp, fido }
    }
}

/// Represents the second password status of an account.
///
/// TODO: Add other additional password types,
/// e.g.  the password that proton pass allows users to set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Record)]
pub struct SecondPasswordStatus {
    /// Whether a mailbox password has been set.
    pub mailbox_password: bool,
}

impl From<PasswordMode> for SecondPasswordStatus {
    fn from(mode: PasswordMode) -> Self {
        match mode {
            PasswordMode::One => Self {
                mailbox_password: false,
            },

            PasswordMode::Two => Self {
                mailbox_password: true,
            },
        }
    }
}
