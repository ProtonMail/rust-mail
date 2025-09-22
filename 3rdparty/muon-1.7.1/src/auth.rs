//! ## Auth
//!
//! This module defines the auth type held by the `muon` client,
//! used to authenticate with the Proton REST API.

use derive_more::{Debug, Display};
use serde::{Deserialize, Serialize};

/// Represents the authentication state of the client.
///
/// This can be in one of four states:
/// - No auth: the client is not authenticated,
/// - External: the auth is managed by an external service (e.g. the browser),
/// - Internal: the auth is managed by the client itself.
/// - Anonymous: the client is authenticated using an unauthenticated session,
///   managed by the client itself.
///
/// In the internal state, the client holds a token that can be used to
/// authenticate with the Proton API. This token can be either a refresh token
/// (which must be refreshed before use) or an access token (which can be used
/// to make authenticated requests).
#[must_use]
#[derive(Debug, Display, Default, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Auth {
    /// No auth.
    #[default]
    #[debug("Auth::None")]
    #[display("None")]
    None,

    /// An external auth, managed by an external service (e.g. the browser).
    #[display("external({user_id}, {uid})")]
    External {
        /// The user ID.
        user_id: String,

        /// The auth UID.
        uid: String,
    },

    /// An internal auth, managed by the client itself.
    #[display("internal({user_id}, {uid})")]
    Internal {
        /// The user ID.
        user_id: String,

        /// The auth UID.
        uid: String,

        /// The auth token(s).
        tok: Tokens,
    },

    /// An anonymous auth, managed by the client itself.
    /// Used for unauthenticated sessions
    #[display("anonymous({uid})")]
    Anonymous {
        /// The auth UID.
        uid: String,
        /// The auth token(s).
        tok: Tokens,
    },
}

impl Auth {
    /// Create a new external auth.
    pub fn external(user_id: impl AsRef<str>, uid: impl AsRef<str>) -> Self {
        Self::External {
            user_id: user_id.as_ref().to_owned(),
            uid: uid.as_ref().to_owned(),
        }
    }

    /// Create a new internal auth.
    pub fn internal(
        user_id: impl AsRef<str>,
        uid: impl AsRef<str>,
        tok: impl Into<Tokens>,
    ) -> Self {
        Self::Internal {
            user_id: user_id.as_ref().to_owned(),
            uid: uid.as_ref().to_owned(),
            tok: tok.into(),
        }
    }

    /// Create a new internal auth.
    pub fn anonymous(uid: impl AsRef<str>, tok: impl Into<Tokens>) -> Self {
        Self::Anonymous {
            uid: uid.as_ref().to_owned(),
            tok: tok.into(),
        }
    }

    /// Update the scopes of the auth.
    ///
    /// This is a convenience method which updates the scopes of the tokens
    /// held by the auth, if any. If no tokens are present, or if the tokens
    /// have no scopes, this method returns `None`.
    pub(crate) fn with_scopes(self, scopes: impl IntoIterator<Item: AsRef<str>>) -> Option<Self> {
        if let Self::Internal { user_id, uid, tok } = self {
            Some(Self::internal(user_id, uid, tok.with_scopes(scopes)?))
        } else if let Self::Anonymous { uid, tok } = self {
            Some(Self::anonymous(uid, tok.with_scopes(scopes)?))
        } else {
            None
        }
    }

    /// Get the auth user ID, if any.
    #[must_use]
    pub fn user_id(&self) -> Option<&str> {
        if let Self::Internal { user_id, .. } = self {
            Some(user_id)
        } else if let Self::External { user_id, .. } = self {
            Some(user_id)
        } else {
            None
        }
    }

    /// Get the auth UID, if any.
    #[must_use]
    pub fn uid(&self) -> Option<&str> {
        if let Self::Internal { uid, .. } = self {
            Some(uid)
        } else if let Self::External { uid, .. } = self {
            Some(uid)
        } else if let Self::Anonymous { uid, .. } = self {
            Some(uid)
        } else {
            None
        }
    }

    /// Get the tokens, if any.
    #[must_use]
    pub fn tokens(&self) -> Option<&Tokens> {
        if let Self::Internal { tok, .. } = self {
            Some(tok)
        } else if let Self::Anonymous { tok, .. } = self {
            Some(tok)
        } else {
            None
        }
    }

    /// Get the refresh token from the tokens, if any.
    ///
    /// This is a convenience method equivalent to
    /// `self.tokens().map(Tokens::ref_tok)`.
    #[must_use]
    pub fn ref_tok(&self) -> Option<&str> {
        self.tokens().map(Tokens::ref_tok)
    }

    /// Get the access token from the tokens, if any.
    ///
    /// This is a convenience method equivalent to
    /// `self.tokens().and_then(Tokens::acc_tok)`.
    #[must_use]
    pub fn acc_tok(&self) -> Option<&str> {
        self.tokens().and_then(Tokens::acc_tok)
    }

    /// Get the scopes from the tokens, if any.
    ///
    /// This is a convenience method equivalent to
    /// `self.tokens().and_then(Tokens::scopes)`.
    #[must_use]
    pub fn scopes(&self) -> Option<&[String]> {
        self.tokens().and_then(Tokens::scopes)
    }

    /// Return whether the auth has the given scope.
    /// This is a convenience method equivalent to
    /// `self.scopes().is_some_and(|scopes| scopes.contains(scope))`.
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes().is_some_and(|s| s.iter().any(|s| s == scope))
    }
}

/// A set of tokens.
///
/// This type represents the tokens held by the client.
/// Depending on the state of the client's auth session, it can be either
/// a single refresh token (which must be refreshed before use) or an access
/// token (and associated refresh token and scopes).
#[derive(Debug, Display, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Tokens {
    /// A single refresh token.
    ///
    /// This token must be refreshed before use;
    /// once refreshed, it becomes an access token.
    #[debug("Tokens::Refresh")]
    #[display("<refresh>")]
    Refresh {
        /// The refresh token's value.
        ref_tok: String,
    },

    /// An access token.
    ///
    /// This token can be used to make authenticated requests to the Proton API.
    /// It is associated with a refresh token, used to get a new access token
    /// when the current one expires, and a set of scopes, which define the
    /// permissions granted by the token.
    #[debug("Tokens::Access")]
    #[display("<access>")]
    Access {
        /// The access token.
        acc_tok: String,

        /// The refresh token.
        ref_tok: String,

        /// The scopes.
        scopes: Vec<String>,
    },
}

impl Tokens {
    /// Create a new refresh token.
    pub fn refresh(ref_tok: impl AsRef<str>) -> Self {
        Self::Refresh {
            ref_tok: ref_tok.as_ref().to_owned(),
        }
    }

    /// Create a new access token.
    pub fn access<T: AsRef<str>>(
        acc_tok: impl AsRef<str>,
        ref_tok: impl AsRef<str>,
        scopes: impl IntoIterator<Item = T>,
    ) -> Self {
        Self::Access {
            acc_tok: acc_tok.as_ref().to_owned(),
            ref_tok: ref_tok.as_ref().to_owned(),
            scopes: scopes.into_iter().map(|s| s.as_ref().to_owned()).collect(),
        }
    }

    /// Update the scopes of the tokens.
    ///
    /// This is a convenience method which updates the scopes of the tokens, if
    /// any. If the tokens have no scopes, this method returns `None`.
    pub(crate) fn with_scopes(self, scopes: impl IntoIterator<Item: AsRef<str>>) -> Option<Self> {
        if let Self::Access {
            acc_tok, ref_tok, ..
        } = self
        {
            Some(Self::access(acc_tok, ref_tok, scopes))
        } else {
            None
        }
    }

    /// Get the refresh token, if any.
    #[must_use]
    pub fn ref_tok(&self) -> &str {
        match self {
            Self::Refresh { ref_tok } => ref_tok,
            Self::Access { ref_tok, .. } => ref_tok,
        }
    }

    /// Get the access token, if any.
    #[must_use]
    pub fn acc_tok(&self) -> Option<&str> {
        if let Self::Access { acc_tok, .. } = self {
            Some(acc_tok)
        } else {
            None
        }
    }

    /// Get the scopes, if any.
    #[must_use]
    pub fn scopes(&self) -> Option<&[String]> {
        if let Self::Access { scopes, .. } = self {
            Some(scopes)
        } else {
            None
        }
    }

    /// Return whether the tokens have the given scope.
    pub fn has_scope(&self, scope: &str) -> bool {
        self.scopes().is_some_and(|s| s.iter().any(|s| s == scope))
    }
}

/// Represents the password mode of an account.
///
/// Note: this is not strictly related to the auth system;
/// it is used to determine whether an account's keys are locked
/// with the primary account password or with a separate password.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PasswordMode {
    /// The account has one password.
    One = 1,

    /// The account has two passwords.
    Two = 2,
}

/// Defines scopes for the Proton API.
pub mod scope {
    /// The `twofactor` scope.
    pub const TWO_FACTOR: &str = "twofactor";

    /// The `loggedin` scope.
    pub const LOGGED_IN: &str = "loggedin";
}
