use crate::test::server::error::{cerr, serr, ServerErr, ServerRes};
use crate::util::IntoIterExt;
use derive_more::{Debug, Display, FromStr};
use futures::lock::Mutex;
use proton_srp;
use proton_srp::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tracing::{debug, info, instrument, trace, warn};
use uuid::Uuid;

/// A static modulus signed by Proton
const MODULUS: &str = "-----BEGIN PGP SIGNED MESSAGE-----\nHash: SHA256\n\nW2z5HBi8RvsfYzZTS7qBaUxxPhsfHJFZpu3Kd6s1JafNrCCH9rfvPLrfuqocxWPgWDH2R8neK7PkNvjxto9TStuY5z7jAzWRvFWN9cQhAKkdWgy0JY6ywVn22+HFpF4cYesHrqFIKUPDMSSIlWjBVmEJZ/MusD44ZT29xcPrOqeZvwtCffKtGAIjLYPZIEbZKnDM1Dm3q2K/xS5h+xdhjnndhsrkwm9U9oyA2wxzSXFL+pdfj2fOdRwuR5nW0J2NFrq3kJjkRmpO/Genq1UW+TEknIWAb6VzJJJA244K/H8cnSx2+nSNZO3bbo6Ys228ruV9A8m6DhxmS+bihN3ttQ==\n-----BEGIN PGP SIGNATURE-----\nVersion: ProtonMail\nComment: https://protonmail.com\n\nwl4EARYIABAFAlwB1j0JEDUFhcTpUY8mAAD8CgEAnsFnF4cF0uSHKkXa1GIa\nGO86yMV4zDZEZcDSJo0fgr8A/AlupGN9EdHlsrZLmTA1vhIx+rOgxdEff28N\nkvNM7qIK\n=q6vu\n-----END PGP SIGNATURE-----";

/// A Dummy PGP Private Key to mock some API calls
const PRIVATE_KEY : &str ="-----BEGIN PGP PRIVATE KEY BLOCK-----\nVersion: ProtonMail\n\nxYYEaKhS8BYJKwYBBAHaRw8BAQdAcYby40cCHtEmywarg+Lq5WTArRDNxcwZ\nBXBA/7090a3+CQMIeR7Cn7SCU45gAAAAAAAAAAAAAAAAAAAAAJvBnCZr1rkB\nAQUq7KVvXJZD/07pp/u8yKKbXDVpZ/8jpWV7XXaHi94gcWF+PG9zGkDGEyGe\n2s07bm90X2Zvcl9lbWFpbF91c2VAZG9tYWluLnRsZCA8bm90X2Zvcl9lbWFp\nbF91c2VAZG9tYWluLnRsZD7CwBEEExYKAIMFgmioUvADCwkHCZBkiZFk6dHS\n80UUAAAAAAAcACBzYWx0QG5vdGF0aW9ucy5vcGVucGdwanMub3JnIbS0SGdm\n+y9QsZhyd41RZIIpybCzQaFAXz0yhSZdKzEDFQoIBBYAAgECGQECmwMCHgEW\nIQT5csiSaJJF4w47zhZkiZFk6dHS8wAAjl0A/1UxrI8VE2BQK/J90YBo+EeX\neTRqwQ8ApsD7VsGf/ARYAP9L6vZ6BOAM5gixrEmiYiLQuUEvNpAn1EzGsOBb\n6jzzAceLBGioUvASCisGAQQBl1UBBQEBB0AgJqTrnC07b4OezbSFtNhsVUqk\n/E9BuQiy3JMzeUpbSgMBCAf+CQMI/Axd0wbLRQpgAAAAAAAAAAAAAAAAAAAA\nANu4JCgiUzjPSeRODVxGopnjp3OKS3zFp/1qOonzY9e+Q2dqZ35yOme8ImrU\nAIAxPu2UH2xLMcK+BBgWCgBwBYJoqFLwCZBkiZFk6dHS80UUAAAAAAAcACBz\nYWx0QG5vdGF0aW9ucy5vcGVucGdwanMub3JnPiiviamygefyd1ZHn3fKq1sZ\ndAIkhVmdSsBYHtzx4BQCmwwWIQT5csiSaJJF4w47zhZkiZFk6dHS8wAAMskA\n/2qzEvvIC0G+MGDHnlPCaWyk9k+OdC3CJey+Kc6t0Qw1AQDY0Xgqmh24bAbl\nRge2NFvN98RSk/Xpr+9NKwE38YttAw==\n=rec3\n-----END PGP PRIVATE KEY BLOCK-----\n";

/// The server's backend.
///
/// This implements all business logic for the server.
#[derive(Debug, Default, Clone)]
pub struct Backend {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Debug, Default)]
struct Inner {
    /// Registered users.
    user: HashMap<UserId, User>,

    /// PGP keys.
    keys: HashMap<KeyId, Key>,

    /// Active auth sessions.
    auth: HashMap<AuthUid, Auth>,

    /// Ongoing SRP challenges.
    srp: HashMap<SrpId, Srp>,
}

impl Backend {
    /// Register a new user.
    ///
    /// This sets up a new user with the given username and password.
    #[instrument(skip(self))]
    pub async fn new_user(&self, name: &str, pass: &str) -> ServerRes<UserId> {
        info!("registering new user");

        // Generate the user's salt.
        let addr = format!("{name}@foo.com");

        let Ok(client_verifier) =
            SRPAuth::generate_verifier(&RPGPVerifier::default(), pass, None, MODULUS)
        else {
            return Err(serr!("failed to generate verifier"));
        };

        let mut lock = self.inner.lock().await;

        let key_id = KeyId::new();
        lock.keys.insert(key_id, Key::new(PRIVATE_KEY.to_owned()));

        let user_id = UserId::new();
        let user = User::new(name, addr, client_verifier, [key_id]);
        lock.user.insert(user_id, user);

        info!(%user_id, "created new user");

        Ok(user_id)
    }

    /// Get a user's ID by their username.
    #[instrument(skip(self))]
    pub async fn get_user_id(&self, name: &str) -> ServerRes<UserId> {
        let lock = self.inner.lock().await;

        let user_id = (lock.user.iter())
            .find_map(|(&id, user)| if user.name == name { Some(id) } else { None })
            .ok_or_else(|| cerr!(422, "no such user {name}"))?;

        Ok(user_id)
    }

    /// Get a user's data.
    #[instrument(skip(self))]
    pub async fn get_user(&self, user_id: UserId) -> ServerRes<User> {
        let lock = self.inner.lock().await;

        let user = (lock.user.get(&user_id))
            .ok_or_else(|| cerr!(422, "no such user"))?
            .to_owned();

        Ok(user)
    }

    /// Get one or more keys by their IDs.
    #[instrument(skip(self))]
    pub async fn get_keys(&self, key_ids: &[KeyId]) -> ServerRes<HashMap<KeyId, Key>> {
        let lock = self.inner.lock().await;

        let mut keys = HashMap::with_capacity(key_ids.len());

        for &key_id in key_ids {
            if let Some(key) = lock.keys.get(&key_id) {
                keys.insert(key_id, key.clone());
            } else {
                return Err(cerr!(422, "no such key {key_id}"));
            }
        }

        Ok(keys)
    }

    /// Begin a new SRP auth session.
    #[instrument(skip(self, verifier))]
    pub async fn new_srp_session(
        &self,
        user_id: UserId,
        verifier: SRPVerifier,
    ) -> ServerRes<(SrpId, Vec<u8>, String)> {
        info!("creating new SRP session");

        // Start dummy login with the verifier from the client above
        let server_client_verifier = ServerClientVerifier::from(&verifier);

        let mut server = ServerInteraction::new_with_modulus_extractor(
            &RPGPVerifier::default(),
            MODULUS,
            &server_client_verifier,
        )
        .map_err(Into::into)
        .map_err(ServerErr::Server)?;

        let server_challenge = server.generate_challenge();

        let mut lock = self.inner.lock().await;

        let srp_id = SrpId::new();
        let srp = Srp::new(server, user_id);
        lock.srp.insert(srp_id, srp);
        info!(%srp_id, "created new SRP session");

        Ok((srp_id, server_challenge.0.to_vec(), MODULUS.to_owned()))
    }

    /// Verify an SRP proof.
    #[instrument(skip(self, ephemeral, proof))]
    pub async fn verify_srp_proof(
        &self,
        user_id: UserId,
        session: &str,
        ephemeral: &[u8],
        proof: &[u8],
    ) -> ServerRes<Vec<u8>> {
        info!("verifying SRP proof");

        let Ok(srp_id) = session.parse() else {
            return Err(cerr!(422, "invalid SRP session"));
        };

        let Some(mut srp) = self.inner.lock().await.srp.remove(&srp_id) else {
            return Err(cerr!(422, "no such SRP session {srp_id}"));
        };

        if srp.user_id != user_id {
            return Err(cerr!(422, "unexpected user {user_id}"));
        }
        let Ok(server_client_proof) =
            ServerClientProof::new_with_bytes(ephemeral.to_vec(), proof.to_vec())
        else {
            return Err(cerr!(401, "invalid proof"));
        };

        let Ok(server_proof) = srp.server.verify_proof(&server_client_proof) else {
            return Err(cerr!(401, "invalid proof"));
        };

        Ok(server_proof.to_vec())
    }

    /// Grant a new auth session for a user.
    #[instrument(skip(self))]
    pub async fn new_auth_session(&self, user_id: UserId) -> ServerRes<AuthUid> {
        info!("creating new auth session");

        let mut lock = self.inner.lock().await;

        let uid = AuthUid::new();
        let scopes = [Scope::Full, Scope::LoggedIn];
        let auth = Auth::new(user_id, scopes);
        lock.auth.insert(uid, auth);
        info!(%uid, "created new auth session");

        Ok(uid)
    }

    /// Grant a new unauth session
    #[instrument(skip(self))]
    pub async fn new_unauth_session(&self) -> ServerRes<AuthUid> {
        info!("creating new auth session");

        let mut lock = self.inner.lock().await;

        let uid = AuthUid::new();
        let scopes = [];
        let auth = Auth::new(UserId::new(), scopes);
        lock.auth.insert(uid, auth);
        info!(%uid, "created new unauth session");

        Ok(uid)
    }

    /// Get an auth session by its ID.
    #[instrument(skip(self))]
    pub async fn get_auth_session(&self, uid: AuthUid) -> ServerRes<Auth> {
        let lock = self.inner.lock().await;

        let auth = (lock.auth.get(&uid))
            .ok_or_else(|| cerr!(401, "no such auth session {uid}"))?
            .to_owned();

        Ok(auth)
    }

    /// Refresh an auth session, if one exists and the token is valid.
    #[instrument(skip(self))]
    pub async fn refresh_auth_session(&self, uid: &str, tok: &str) -> ServerRes<AuthUid> {
        info!("refreshing auth session");

        let mut lock = self.inner.lock().await;

        let Ok(uid): Result<AuthUid, _> = uid.parse() else {
            return Err(cerr!(422, "invalid UID"));
        };

        let Ok(tok): Result<RefTok, _> = tok.parse() else {
            return Err(cerr!(422, "invalid refresh token"));
        };

        let Some(auth) = lock.auth.remove(&uid) else {
            return Err(cerr!(401, "no such auth session {uid}"));
        };

        if auth.reftok == tok {
            let scopes = [Scope::Full, Scope::LoggedIn];
            let auth = auth.refresh(scopes);
            lock.auth.insert(uid, auth);
            info!(%uid, "refreshed auth session");

            Ok(uid)
        } else {
            lock.auth.insert(uid, auth);
            warn!(%uid, "auth session not refreshed");

            Err(cerr!(401, "invalid refresh token"))
        }
    }

    /// Verify an auth token.
    #[instrument(skip(self))]
    pub async fn verify_auth(&self, uid: &str, tok: &str) -> ServerRes<(UserId, Vec<Scope>)> {
        debug!("verifying auth token");

        let lock = self.inner.lock().await;

        let Ok(uid): Result<AuthUid, _> = uid.parse() else {
            return Err(cerr!(422, "invalid UID"));
        };

        let Ok(tok): Result<AccTok, _> = tok.parse() else {
            return Err(cerr!(422, "invalid auth token"));
        };

        let Some(auth) = lock.auth.get(&uid) else {
            return Err(cerr!(401, "no such auth session {uid}"));
        };

        let Some((acctok, scopes)) = &auth.acctok else {
            return Err(cerr!(401, "no access token"));
        };

        if acctok == &tok {
            trace!(%uid, "verified auth token");
            Ok((auth.user_id, scopes.iter().copied().collect()))
        } else {
            warn!(%uid, have = %tok, want = %acctok, "invalid auth token");
            Err(cerr!(401, "invalid auth token"))
        }
    }

    /// Expire a user's auth session.
    #[instrument(skip(self))]
    pub async fn expire_auth(&self, user_id: UserId) -> ServerRes<()> {
        info!("expiring auth sessions");

        let mut inner = self.inner.lock().await;

        if !inner.user.contains_key(&user_id) {
            return Err(cerr!(422, "no such user {user_id}"));
        }

        for (uid, auth) in &mut inner.auth {
            if auth.user_id == user_id {
                info!(%uid, "removing access token from auth session");
                auth.acctok = None;
            }
        }

        Ok(())
    }

    /// Expire all auth sessions.
    #[instrument(skip(self))]
    pub async fn expire_all(&self) -> ServerRes<()> {
        info!("expiring all auth sessions");

        let mut inner = self.inner.lock().await;

        for (uid, auth) in &mut inner.auth {
            info!(%uid, "removing access token from auth session");
            auth.acctok = None;
        }

        Ok(())
    }
}

/// A user ID.
#[derive(Debug, Display, FromStr, Clone, Copy, PartialEq, Eq, Hash)]
#[debug("{self}")]
pub struct UserId(Uuid);

impl UserId {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// A user.
#[derive(Debug, Clone)]
pub struct User {
    /// The user's name.
    pub name: String,

    /// The user's email.
    pub email: String,

    /// The user's verifier.
    pub verifier: proton_srp::SRPVerifier,

    /// The user's PGP key.
    pub keys: Vec<KeyId>,
}

impl User {
    fn new(
        name: impl Into<String>,
        email: impl Into<String>,
        verifier: proton_srp::SRPVerifier,
        keys: impl IntoIterator<Item = KeyId>,
    ) -> Self {
        Self {
            name: name.into(),
            email: email.into(),
            verifier,
            keys: keys.into_iter().collect(),
        }
    }
}

/// A PGP key ID.
#[derive(Debug, Display, FromStr, Clone, Copy, PartialEq, Eq, Hash)]
#[debug("{self}")]
pub struct KeyId(Uuid);

impl KeyId {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// A PGP key.
#[allow(warnings)]
#[derive(Debug, Clone)]
pub struct Key {
    /// The private key.
    pub key: String,

    /// The key's token, if any.
    pub token: Option<String>,

    /// The key's signature, if any.
    pub signature: Option<String>,

    /// Whether this key is active.
    pub active: bool,
}

impl Key {
    fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            token: None,
            signature: None,
            active: true,
        }
    }
}

/// An auth session ID.
#[derive(Debug, Display, FromStr, Clone, Copy, PartialEq, Eq, Hash)]
#[debug("{self}")]
pub struct AuthUid(Uuid);

impl AuthUid {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// An auth access token.
#[derive(Debug, Display, FromStr, Clone, Copy, PartialEq, Eq)]
#[debug("{self}")]
pub struct AccTok(Uuid);

impl AccTok {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// An auth refresh token.
#[derive(Debug, Display, FromStr, Clone, Copy, PartialEq, Eq)]
#[debug("{self}")]
pub struct RefTok(Uuid);

impl RefTok {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// The data associated with an auth session.
#[derive(Debug, Clone)]
pub struct Auth {
    pub user_id: UserId,
    pub reftok: RefTok,
    pub acctok: Option<(AccTok, HashSet<Scope>)>,
}

impl Auth {
    fn new(user_id: UserId, scopes: impl IntoIterator<Item = Scope>) -> Self {
        Self {
            user_id,
            reftok: RefTok::new(),
            acctok: Some((AccTok::new(), scopes.into_iter().collect())),
        }
    }

    fn refresh(self, scopes: impl IntoIterator<Item = Scope>) -> Self {
        Self {
            user_id: self.user_id,
            reftok: RefTok::new(),
            acctok: Some((AccTok::new(), scopes.into_set())),
        }
    }
}

/// An auth scope.
#[derive(Debug, Display, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    #[display("full")]
    Full,

    #[display("loggedin")]
    LoggedIn,
}

/// An SRP challenge ID.
#[derive(Debug, Display, FromStr, Clone, Copy, PartialEq, Eq, Hash)]
#[debug("{self}")]
pub struct SrpId(Uuid);

impl SrpId {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// An ongoing SRP challenge.
#[derive(Debug)]
struct Srp {
    /// The client-to-server interaction for a challenge
    server: ServerInteraction,

    /// The ID of the user this challenge is for.
    user_id: UserId,
}

impl Srp {
    fn new(server: ServerInteraction, user_id: UserId) -> Self {
        Self { server, user_id }
    }
}
