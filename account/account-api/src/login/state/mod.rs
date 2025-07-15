use crate::AccountApi;
use crate::login::LoginError;
use crate::login::state::complete::Complete;
use crate::login::state::want_login::WantLogin;
use crate::login::state::want_mbp::WantMbp;
use crate::login::state::want_new_password::WantNewPassword;
use crate::login::state::want_tfa::{TfaFlow, WantTfa};
use crate::prelude::AuthInput;
use crate::shared::SecureString;
use crate::shared::challenge::{Behavior, ChallengeInfo};
use crate::shared::crypto::{NewAddrKey, NewUserKey, SharedCryptoError};
use derive_more::{Debug, From};
use futures::TryFutureExt;
use itertools::Itertools;
use muon::client::flow::LoginFlowData;
use muon::rest::auth::v4::fido2;
use proton_core_api::auth::UserKeySecret;
use proton_core_api::services::observability::ObservabilityRecorder;
use proton_core_api::services::proton::{Address, AddressId, ProtonCore, SessionId, User, UserId};
use proton_core_api::session::{Session, SessionParts};
use proton_core_api::store::{MbpMode, UserData};
use proton_crypto_account::keys::{LockedKey, UnlockedUserKey, UserKeys};
use proton_crypto_account::proton_crypto;
use proton_crypto_account::proton_crypto::crypto::PGPProviderSync;
use proton_crypto_account::proton_crypto::srp::SRPProvider;
use proton_crypto_account::salts::{Salt, Salts};
use secrecy::SecretString;
use std::collections::HashMap;
use want_qr_confirmation::WantQrConfirmation;

pub mod complete;
mod want_login;
mod want_mbp;
mod want_new_password;
pub mod want_qr_confirmation;
mod want_tfa;

/// Represents the possible states that the login flow can be in,
/// ensuring only valid transitions between states are possible.
#[derive(Debug, From)]
pub enum State {
    /// The flow is waiting for the user to provide their login credentials.
    #[debug("WantLogin")]
    WantLogin(WantLogin),

    /// A recoverable error occurred during the `WantLogin` state.
    #[debug("LoginRetry")]
    LoginRetry,

    /// The flow is waiting for the user to provide a 2FA token.
    #[debug("WantTfa")]
    WantTfa(WantTfa),

    /// A recoverable error occurred during the `WantTfa` state.
    #[debug("TfaRetry")]
    TfaRetry(UserId, SessionId, SecureString, MbpMode, Option<fido2::Response>),

    /// An error occurred during the `WantTfa` state.
    #[debug("TfaError")]
    TfaError,

    /// The flow is waiting for the user to provide their mailbox password.
    #[debug("WantMbp")]
    WantMbp(WantMbp),

    /// A recoverable error occurred during the `WantMbp` state.
    #[debug("MbpRetry")]
    MbpRetry(UserId, SessionId),

    /// The flow is waiting for the user to provide a new password (for temporary password users).
    #[debug("WantNewPassword")]
    WantNewPassword(WantNewPassword),

    /// This device is the Target device and is waiting for Origin to scan and therefore confirm the login
    #[debug("WantQrConfirmation")]
    WantQrConfirmation(WantQrConfirmation),

    /// The flow is complete.
    #[debug("Complete")]
    Complete(Complete),

    /// Invalid state, cannot be used.
    #[debug("Invalid")]
    Invalid,
}

/// Public actions that can be taken on the state.
impl State {
    /// Attempt to login with the provided credentials.
    pub async fn login_with_credentials(
        self,
        user: String,
        pass: SecureString,
        user_behavior: Option<Behavior>,
    ) -> Result<Self, (Self, LoginError)> {
        if let Self::WantLogin(state) = self {
            Ok(state
                .login_with_credentials(user, pass, user_behavior)
                .await?)
        } else {
            Err((self, LoginError::InvalidState))
        }
    }

    /// Generates a QR code for user sign-in, optionally including an encryption key.
    ///
    /// This method initiates a code-based authentication flow and constructs a QR code string
    /// in the format: `version:user_code:encryption_key_base64:client_id`.
    /// If an encryption key is required, a secure 32-byte key is generated and encoded in Base64.
    /// The resulting state includes the QR code, user code, and encryption key (if applicable) for further processing.
    ///
    /// # Arguments
    /// * `need_encryption_key` - If `true`, generates a 32-byte encryption key; otherwise, uses an empty default.
    pub async fn generate_sign_in_qr_code(
        self,
        need_encryption_key: bool,
    ) -> Result<Self, (Self, LoginError)> {
        if let Self::WantLogin(state) = self {
            state
                .generate_sign_in_qr_code(need_encryption_key)
                .await
                .map_err(|err| (Self::LoginRetry, err))
        } else {
            Err((self, LoginError::InvalidState))
        }
    }

    /// Verifies host device confirmation for QR code login and completes the authentication process.
    ///
    /// This method waits for host device confirmation of the QR code login, decodes the payload using
    /// the provided encryption key, fetches user information, validates the passphrase, and stores user
    /// data. On success, it constructs a completed authentication state with session details.
    pub async fn check_host_device_confirmation(self) -> Result<Self, (Self, LoginError)> {
        if let Self::WantQrConfirmation(state) = self {
            state
                .check_host_device_confirmation()
                .await
                .map_err(|err| (Self::LoginRetry, err))
        } else {
            Err((self, LoginError::InvalidState))
        }
    }

    /// Attempt to migrate existing alive session from
    /// the Legacy version of the application.
    pub async fn migrate(
        self,
        client: muon::Client,
        user: UserData,
        data: LoginFlowData,
        refresh_token: SecretString,
    ) -> Result<Self, (Self, LoginError)> {
        let Self::WantLogin(state) = self else {
            return Err((self, LoginError::InvalidState));
        };

        state.migrate(client, user, data, refresh_token).await
    }

    /// Attempt to submit a TOTP code.
    pub async fn submit_totp(self, code: String) -> Result<Self, (Self, LoginError)> {
        if let Self::WantTfa(state) = self {
            Ok(state.submit_totp(code).await?)
        } else {
            Err((self, LoginError::InvalidState))
        }
    }

    /// Attempt to submit a FIDO code.
    #[allow(unused)]
    pub async fn submit_fido(self, fido_data: fido2::Request) -> Result<Self, (Self, LoginError)> {
        if let Self::WantTfa(state) = self {
            Ok(state.submit_fido(fido_data).await?)
        } else {
            Err((self, LoginError::InvalidState))
        }
    }

    /// Attempt to submit a mailbox password.
    pub async fn submit_mbp(self, pass: SecureString) -> Result<Self, (Self, LoginError)> {
        if let Self::WantMbp(state) = self {
            Ok(state.submit_mbp(pass).await?)
        } else {
            Err((self, LoginError::InvalidState))
        }
    }

    /// Attempt to submit a new password.
    pub async fn submit_new_password(
        self,
        new_pass: SecureString,
    ) -> Result<Self, (Self, LoginError)> {
        if let Self::WantNewPassword(state) = self {
            Ok(state.submit_new_password(new_pass).await?)
        } else {
            Err((self, LoginError::InvalidState))
        }
    }

    /// Attempt to take the completed session from the flow.
    pub fn into_session(self) -> Result<Session, LoginError> {
        if let Self::Complete(state) = self {
            Ok(state.into_session())
        } else {
            Err(LoginError::InvalidState)
        }
    }

    /// Get the user ID of the user that has (or is in the process of) logging in.
    pub fn user_id(&self) -> Result<&UserId, LoginError> {
        let state: &dyn HasUserId = match self {
            Self::WantTfa(state) => state,
            Self::WantMbp(state) => state,
            Self::WantNewPassword(state) => state,
            Self::Complete(state) => state,

            _ => return Err(LoginError::InvalidState),
        };

        Ok(state.user_id())
    }

    /// Get the session ID that has been (or is in the process of) being created.
    pub fn session_id(&self) -> Result<&SessionId, LoginError> {
        let state: &dyn HasSessionId = match self {
            Self::WantTfa(state) => state,
            Self::WantMbp(state) => state,
            Self::WantNewPassword(state) => state,
            Self::Complete(state) => state,

            _ => return Err(LoginError::InvalidState),
        };

        Ok(state.session_id())
    }
}

/// Public entrypoints for creating new states.
impl State {
    /// Create a `WantLogin` state.
    #[must_use]
    pub fn new(
        client: muon::Client,
        parts: SessionParts,
        challenge_info: Option<ChallengeInfo>,
    ) -> Self {
        Self::want_login(client, parts, challenge_info)
    }

    /// Create a `WantTfa` state from a resumed login flow.
    #[must_use]
    pub fn new_from_tfa(
        client: muon::Client,
        parts: SessionParts,
        user_id: UserId,
        session_id: SessionId,
        pass: SecureString,
        mode: MbpMode,
        fido_details: Option<fido2::Response>,
    ) -> Self {
        let data = StateData {
            parts,
            user_id,
            session_id,
            observability: ObservabilityRecorder::default(),
        };

        Self::want_tfa(client.auth().into(), data, pass, mode, fido_details)
    }

    /// Create a `WantMbp` state from a resumed login flow.
    #[must_use]
    pub fn new_from_mbp(
        client: muon::Client,
        parts: SessionParts,
        user_id: UserId,
        session_id: SessionId,
    ) -> Self {
        let data = StateData {
            parts,
            user_id,
            session_id,
            observability: ObservabilityRecorder::default(),
        };

        Self::want_mbp(client, data)
    }
}

/// Private entrypoints for creating new states.
impl State {
    /// Create a `WantLogin` state.
    fn want_login(
        client: muon::Client,
        parts: SessionParts,
        challenge_info: Option<ChallengeInfo>,
    ) -> Self {
        WantLogin::new(client, parts, challenge_info).into()
    }

    /// Create a `WantTfa` state.
    fn want_tfa(flow: TfaFlow, data: StateData, pass: SecureString, mode: MbpMode, fido_details: Option<fido2::Response>) -> Self {
        WantTfa::new(flow, data, pass, mode, fido_details).into()
    }

    /// Create a `WantMbp` state.
    fn want_mbp(client: muon::Client, data: StateData) -> Self {
        WantMbp::new(client, data).into()
    }

    /// Create a `WantNewPassword` state.
    fn want_new_password(client: muon::Client, data: StateData, user: User) -> Self {
        WantNewPassword::new(client, data, user).into()
    }

    /// Inspect the user after successful authentication and determine the appropriate next step.
    async fn inspect_user(
        client: muon::Client,
        data: StateData,
        pass: SecureString,
        mode: MbpMode,
    ) -> Result<Self, LoginError> {
        // Fetch user info.
        let user = client
            .get_users()
            .map_ok(|res| res.user)
            .map_err(LoginError::UserFetch)
            .await?;

        // Is the user forbidden from logging in?
        if user.flags.no_login {
            data.parts.store.write().await.clear_account().await?;
            return Err(LoginError::NoLogin);
        }

        // Does the user have a proton address?
        if user.flags.no_proton_address && !user.flags.has_a_byoe_address {
            data.parts.store.write().await.clear_account().await?;
            return Err(LoginError::NoAddress);
        }

        // Check if user has temporary password - transition to WantNewPassword
        if user.flags.has_temporary_password {
            return Ok(Self::want_new_password(client, data, user));
        }

        // Check if user has mailbox password - transition to WantMbp
        match mode {
            MbpMode::One => Self::finalize(client, data, pass).await,
            MbpMode::Two => Ok(Self::want_mbp(client, data)),
        }
    }

    /// Complete the finalization with key unlocking and storage.
    async fn finalize(
        client: muon::Client,
        data: StateData,
        pass: SecureString,
    ) -> Result<Self, LoginError> {
        // Initialize the crypto providers.
        let srp = proton_crypto::new_srp_provider();
        let pgp = proton_crypto::new_pgp_provider();

        // Fetch user info.
        let mut user = client
            .get_users()
            .map_ok(|res| res.user)
            .map_err(LoginError::UserFetch)
            .await?;

        // Fetch user addresses.
        let mut addr = ProtonCore::get_addresses(&client)
            .map_ok(|res| res.addresses)
            .map_err(LoginError::AddressFetch)
            .await?;

        // Does the user have a key?
        if user.keys.as_ref().is_empty() {
            if want_key_setup(&user) {
                (user, addr) = Self::setup_keys(&srp, &pgp, &client, &addr, &pass).await?;
            } else {
                return Err(LoginError::UserKeySetupAborted);
            }
        }

        // Fetch the user's key salts.
        let salts = client
            .get_keys_salts()
            .map_ok(|res| res.key_salts)
            .map_err(LoginError::KeySecretSaltFetch)
            .await?;

        // Build the salts object.
        let salts = Salts::new(salts.into_iter().map(|salt| Salt {
            id: salt.id.into_inner().into(),
            key_salt: salt.key_salt.map(Into::into),
        }));

        // Derive the key secret to unlock the user keys.
        let user_key_pass = if let Some(key) = user.keys.primary() {
            salts.salt_for_key(&srp, &key.id, pass.as_bytes())?
        } else {
            return Err(LoginError::MissingPrimaryKey);
        };

        // Unlock the user keys.
        let user_keys = match user.keys.unlock(&pgp, &user_key_pass) {
            res if res.unlocked_keys.is_empty() => Err(LoginError::KeySecretDecryption)?,
            res => res.unlocked_keys,
        };

        // Get the primary user key.
        let user_key = (user.keys.primary())
            .and_then(|key| user_keys.iter().find(|k| k.id == key.id))
            .ok_or(LoginError::MissingPrimaryKey)?;

        // Do all the user's addresses have keys?
        if addr.iter().any(|addr| addr.keys.as_ref().is_empty()) {
            if want_key_setup(&user) {
                Self::setup_address_keys(&pgp, &client, user_key, &addr).await?;
            } else {
                return Err(LoginError::AddressKeySetupAborted);
            }
        }

        // Save user data to store
        (data.parts.store.write().await)
            .set_user_data(UserData {
                username: user.name.clone().unwrap_or_default(),
                display_name: user.display_name.clone().unwrap_or_default(),
                primary_addr: user.email.clone(),
                key_secret: UserKeySecret(user_key_pass.clone()),
            })
            .await?;

        Ok(Complete::new(client, data, Some(user)).into())
    }

    /// Finalize login flow for the migration.
    async fn finalize_migration(
        client: muon::Client,
        data: StateData,
        user_data: UserData,
    ) -> Result<Self, LoginError> {
        data.parts
            .store
            .write()
            .await
            .set_user_data(user_data)
            .await?;

        Ok(Complete::new(client, data, None).into())
    }

    /// Set up a user key for a user that doesn't have any keys.
    async fn setup_keys<S: SRPProvider, P: PGPProviderSync>(
        srp: &S,
        pgp: &P,
        client: &muon::Client,
        addr: &[Address],
        pass: &str,
    ) -> Result<(User, Vec<Address>), LoginError> {
        use crate::requests::{AddressKeyInput, AsyncUserInitialization, SetupKeysRequest};

        let user_key = NewUserKey::init(srp, pgp, pass)
            .map_err(|e| LoginError::UserKeySetup(e.to_string()))?;

        let addr_keys: HashMap<AddressId, NewAddrKey> = addr
            .iter()
            .map(|addr| Ok((addr.id.clone(), user_key.init_addr(pgp, &addr.email)?)))
            .try_collect()
            .map_err(|e: SharedCryptoError| LoginError::AddressKeySetup(e.to_string()))?;

        let res = (client)
            .get_auth_modulus()
            .await
            .map_err(|e| LoginError::UserKeySetup(e.to_string()))?;

        let ver = srp
            .generate_client_verifier(pass, &res.modulus)
            .map_err(|e| LoginError::UserKeySetup(e.to_string()))?;

        let address_keys = addr_keys
            .into_iter()
            .map(|(id, key)| AddressKeyInput::new(id.as_str(), &key.key, &key.skl))
            .collect();

        let auth = AuthInput {
            version: ver.version,
            modulus_id: res.modulus_id,
            salt: ver.salt,
            verifier: ver.verifier,
        };

        let request = SetupKeysRequest {
            auth,
            primary_key: user_key.key.private_key.to_string(),
            key_salt: user_key.salt.to_string(),
            address_keys,
            encrypted_secret: None,
            org_primary_user_key: None,
            org_activation_token: None,
        };

        let _ = client
            .setup_keys(AsyncUserInitialization::CalledByClient, request)
            .await
            .map_err(|e| LoginError::UserKeySetup(e.to_string()))?;

        let user = client
            .get_users()
            .map_ok(|res| res.user)
            .map_err(LoginError::UserFetch)
            .await?;

        let addresses = ProtonCore::get_addresses(client)
            .map_ok(|res| res.addresses)
            .map_err(LoginError::AddressFetch)
            .await?;

        Ok((user, addresses))
    }

    /// Set up a new address for an external account that doesn't have any addresses.
    #[allow(unused)]
    async fn setup_address(client: &muon::Client, user: &User) -> Result<(), LoginError> {
        use crate::requests::PostAddressesSetupRequest;

        let domains = AccountApi::get_available_domains(client, None)
            .map_err(|e| LoginError::AddressSetup(e.to_string()))
            .await?
            .domains;

        let request = PostAddressesSetupRequest {
            domain: domains.into_iter().next().unwrap_or_default(),
            display_name: user.display_name.clone(),
            signature: None,
            member_id: None,
            requester_member_id: None,
            address_list: vec![user.email.clone()],
        };

        AccountApi::setup_address(client, request)
            .map_err(|e| LoginError::AddressSetup(e.to_string()))
            .await?;

        Ok(())
    }

    /// Set up keys for all addresses that don't have any keys.
    async fn setup_address_keys<P: PGPProviderSync>(
        pgp: &P,
        client: &muon::Client,
        user_key: &UnlockedUserKey<P>,
        addresses: &[Address],
    ) -> Result<Vec<Address>, LoginError> {
        for address in addresses {
            if address.keys.as_ref().is_empty() {
                Self::setup_address_key(pgp, client, user_key, address).await?;
            }
        }

        let addresses = ProtonCore::get_addresses(client)
            .map_ok(|res| res.addresses)
            .map_err(LoginError::AddressFetch)
            .await?;

        Ok(addresses)
    }

    /// Set up keys for an address that doesn't have any keys.
    async fn setup_address_key<P: PGPProviderSync>(
        pgp: &P,
        client: &muon::Client,
        user_key: &UnlockedUserKey<P>,
        address: &Address,
    ) -> Result<(), LoginError> {
        use crate::requests::{CreateAddressKeyRequest, SignedKeyList};

        let addr_key = NewAddrKey::init(pgp, user_key, &address.email)
            .map_err(|_| LoginError::KeySecretDecryption)?;

        let token = (addr_key.key.token)
            .map(|t| t.to_string())
            .unwrap_or_default();

        let signature = (addr_key.key.signature)
            .map(|s| s.to_string())
            .unwrap_or_default();

        let signed_key_list = SignedKeyList {
            data: addr_key.skl.data.to_string(),
            signature: addr_key.skl.signature.to_string(),
        };

        // TODO: use address ID as forwarding ID?
        let request = CreateAddressKeyRequest {
            address_id: address.id.to_string(),
            private_key: addr_key.key.private_key.to_string(),
            primary: 1,
            address_forwarding_id: None,
            token,
            signature,
            signed_key_list,
        };

        let _ = AccountApi::create_address_key(client, request)
            .map_err(|e| LoginError::AddressKeySetup(e.to_string()))
            .await?
            .key;

        Ok(())
    }
}

pub(crate) struct StateData {
    parts: SessionParts,
    user_id: UserId,
    session_id: SessionId,
    observability: ObservabilityRecorder,
}

/// A trait for states in which the user ID is known.
trait HasUserId {
    fn user_id(&self) -> &UserId;
}

/// A trait for states in which the auth ID is known.
trait HasSessionId {
    fn session_id(&self) -> &SessionId;
}

/// A helper trait for working with user keys.
trait UserKeysExt {
    fn primary(&self) -> Option<&LockedKey>;
}

impl UserKeysExt for UserKeys {
    fn primary(&self) -> Option<&LockedKey> {
        self.as_ref().iter().find(|&key| key.primary)
    }
}

fn want_key_setup(user: &User) -> bool {
    // Non-private users should have keys created by the org -- no key setup needed.
    if !user.private {
        return false;
    }

    // Users with BYOE addresses should already have keys which were created when the address was created.
    if user.flags.has_a_byoe_address {
        return false;
    }

    // Allow key setup for users with temporary passwords
    true
}
