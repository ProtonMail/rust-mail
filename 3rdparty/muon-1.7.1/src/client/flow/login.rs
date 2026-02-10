use crate::client::{Client, Fingerprint};
use crate::auth::{scope, Auth};
use crate::flow::{AuthScopeErr, AuthStateErr, FlowErr, SrpServerProofErr, UserIdErr};
use crate::http::{HttpReqExt, POST};
use crate::rest::auth;
use crate::rest::auth::v4::fido2;
use crate::{PasswordMode, Result, Tokens};
use futures::FutureExt;
use proton_srp::{RPGPVerifier, SRPAuth, SRPError, SRPProofB64};

/// Additional data returned by the login flow.
#[derive(Debug)]
pub struct LoginFlowData {
    /// The user ID.
    pub user_id: String,

    /// The user's unique ID.
    pub session_id: String,

    /// The password mode.
    pub password_mode: PasswordMode,
}

/// A login flow.
///
/// This guides the user through the process of logging in to the Proton API.
#[must_use]
#[derive(Debug)]
pub enum LoginFlow {
    /// The login flow is complete.
    ///
    /// The client is now authenticated and can be used to make requests.
    /// Information about the account and session is returned in addition to
    /// the authenticated client.
    Ok(Client, LoginFlowData),

    /// A two-factor code is needed.
    ///
    /// The client is partially authenticated and needs a 2FA code to proceed.
    /// Information about the account and session is returned in addition to
    /// the continuation of the login flow.
    TwoFactor(LoginTwoFactorFlow, LoginFlowData),

    /// The login process couldn't proceed
    Failed {
        /// The client originating the login flow
        client: Client,

        /// The reason why the flow couldn't proceed
        reason: FlowErr,
    },
}

impl LoginFlow {
    /// Login Flow entry point.
    ///
    /// The client must be un-authenticated to pursue. This method will begin
    /// the login flow and will either return:
    /// - [`LoginFlow::Ok`] if the user-pass login is sufficient
    /// - [`LoginFlow::TwoFactor`] if 2FA is needed to login
    /// - [`LoginFlow::Failed`] if the login process failed
    pub(super) async fn new(client: Client, username: &str, password: &str) -> Self {
        Self::__new(client, username, password, None).await
    }

    #[deprecated = "Use new instead. Passing the extra_info is deprecated. Use InfoProvider to provide the fingerprint."]
    #[allow(deprecated)]
    pub(super) async fn new_with_extra(
        client: Client,
        username: &str,
        password: &str,
        extra_info: LoginExtraInfo,
    ) -> Self {
        Self::__new(client, username, password, Some(extra_info)).await
    }

    #[allow(deprecated)]
    async fn __new(
        client: Client,
        username: &str,
        password: &str,
        extra_info: Option<LoginExtraInfo>,
    ) -> Self {
        debug_assert!(!client.is_authenticated().await);

        let Client { stores, .. } = &client;

        info!(%username, "beginning login flow");
        let req = return_variant_on_error!(
            POST!("/auth/v4/info").body_json(auth_info_post_req(username)),
            client,
            Self::Failed
        );
        
        let res = return_variant_on_error!(client.send(req).await, client, Self::Failed);
        let res = return_variant_on_error!(res.ok(), client, Self::Failed);
        let res: auth::v4::info::PostRes =
            return_variant_on_error!(res.into_body_json(), client, Self::Failed);

        info!(session = %res.session, "generating SRP proofs");
        let proofs = return_variant_on_error!(gen_proofs(password, &res), client, Self::Failed);

        info!(session = %res.session, "sending auth request");

        // Here we choose fingerprints. If the deprecated api is used we choose the fingerprint passed with LoginExtraInfo (deprecated). 
        // Else we choose the fingerprint passed in with the new api.
        let provider_fingerprint = match client.provider {
            Some(ref provider) => provider.fingerprint().await,
            None => None,
        };

        let fingerprint = match extra_info {
            Some(info) => match info.fingerprint {
                Some(fingerprint) => { 
                    warn!("Warning: with_fingerprint is deprecated. Pass a provider to the muon client using with_info_provider. Muon will use this provider to ask for the fingerprint when needed.");
                    Some(fingerprint.to_owned()) 
                },
                None => provider_fingerprint
            },
            None => provider_fingerprint
        };

        let req = return_variant_on_error!(
            POST!("/auth/v4").body_json(auth_post_req(username, &res.session, &proofs, fingerprint)),
            client,
            Self::Failed
        );

        let res = return_variant_on_error!(client.send(req).await, client, Self::Failed);
        let res = return_variant_on_error!(res.ok(), client, Self::Failed);
        let res: auth::v4::PostRes =
            return_variant_on_error!(res.into_body_json(), client, Self::Failed);

        info!(uid = %res.auth.uid, "checking server proof");
        if !proofs.compare_server_proof(&res.server_proof) {
            return LoginFlow::Failed {
                client,
                reason: SrpServerProofErr.into(),
            };
        }

        info!(uid = %res.auth.uid, "determining password mode");
        let password_mode = match res.password_mode {
            auth::v4::PasswordMode::One => PasswordMode::One,
            auth::v4::PasswordMode::Two => PasswordMode::Two,
        };

        info!(uid = %res.auth.uid, "determining user ID");
        let Some(user_id) = res.auth.user_id else {
            return LoginFlow::Failed {
                client,
                reason: UserIdErr.into(),
            };
        };

        info!(uid = %res.auth.uid, "building auth object");
        let auth = Auth::internal(
            &user_id,
            &res.auth.uid,
            Tokens::access(
                res.auth.access_token,
                res.auth.refresh_token,
                res.auth.scopes,
            ),
        );

        info!(uid = ?auth.uid(), "building login data");
        let data = LoginFlowData {
            user_id,
            session_id: res.auth.uid,
            password_mode,
        };

        info!(uid = ?auth.uid(), "checking for complete auth");
        if auth.has_scope(scope::LOGGED_IN) {
            info!(%auth, "auth complete, storing tokens");
            stores.set_auth(auth).await;

            return Self::Ok(client, data);
        }

        info!(uid = ?auth.uid(), "checking for 2FA");
        if auth.has_scope(scope::TWO_FACTOR) {
            info!(%auth, "auth requires 2FA, continuing flow");
            stores.set_auth(auth).await;

            let totp = res.tfa.totp_enabled();
            let fido_details = res.tfa.fido_details();
            let flow = LoginTwoFactorFlow::new(client, totp, fido_details);

            return Self::TwoFactor(flow, data);
        }

        LoginFlow::Failed {
            client,
            reason: AuthScopeErr.into(),
        }
    }
}

/// Additional options for the login flow.
#[must_use]
#[derive(Debug, Default)]
#[deprecated = "Instead of LoginExtraInfo, pass a provider to the muon client using with_info_provider. Muon will use this provider to ask for the fingerprint when needed."]
pub struct LoginExtraInfo {
    fingerprint: Option<Fingerprint>,
}

#[allow(deprecated)]
impl LoginExtraInfo {
    /// Create a new builder.
    pub fn builder() -> LoginExtraInfoBuilder {
        LoginExtraInfoBuilder::default()
    }
}

/// Builder for additional options for the login flow.
#[must_use]
#[derive(Debug, Default)]
#[deprecated = "Instead of LoginExtraInfoBuilder, pass a provider to the muon client using with_info_provider. Muon will use this provider to ask for the fingerprint when needed."]
pub struct LoginExtraInfoBuilder {
    fingerprint: Option<Fingerprint>,
}

#[allow(deprecated)]
impl LoginExtraInfoBuilder {
    /// Build the extra options for the login flow.
    pub fn build(&self) -> LoginExtraInfo {
        LoginExtraInfo {
            fingerprint: self.fingerprint.to_owned(),
        }
    }

    /// Include the given fingerprint payload,
    /// used when logging in, for anti-abuse.
    #[deprecated = "Instead of using this function, pass a provider to the muon client using with_info_provider. Muon will use this provider to ask for the fingerprint when needed."]
    pub fn with_fingerprint(mut self, fingerprint: Fingerprint) -> Self {
        self.fingerprint = Some(fingerprint);
        self
    }
}

/// The two-factor authentication step of the login flow.
///
/// This *MUST NOT* be created somewhere else than the [`LoginFlow`].
#[must_use]
#[derive(Debug)]
pub struct LoginTwoFactorFlow {
    client: Client,
    totp: bool,
    fido: Option<fido2::Response>,

}

impl LoginTwoFactorFlow {
    fn new(client: Client, totp: bool, fido: Option<fido2::Response>) -> Self {
        Self { client, totp, fido }
    }

    pub(crate) async fn from_totp(client: Client, code: impl AsRef<str>) -> Result<Client> {
        LoginTwoFactorFlow::new(client, true, None)
            .totp(code)
            .await
    }

    /// Creates a login flow from the 2FA stage using a FIDO2 device.
    pub async fn fido(self, fido_data: fido2::Request) -> Result<Client> {
        Self::from_fido(self.client, fido_data).await
    }

    /// Creates a login flow from the 2FA stage using a FIDO2 device.
    pub(crate) async fn from_fido(client: Client, fido_data: fido2::Request) -> Result<Client> {
        let request_body = auth_tfa_post_fido2_req(fido_data);

        Self::send_2fa_request(client, request_body, "sending FIDO2 request").await
    }

    /// Return whether TOTP is enabled.
    #[must_use]
    pub fn has_totp(&self) -> bool {
        self.totp
    }

    /// Return whether FIDO2 is enabled.
    #[must_use]
    pub fn fido_details(&self) -> Option<&fido2::Response> {
        self.fido.as_ref()
    }

    
    /// Complete the two-factor authentication flow by providing a TOTP code.
    ///
    /// # Errors
    ///
    /// Returns an error if the flow fails.
    pub async fn totp(self, code: impl AsRef<str>) -> Result<Client> {
        self.totp_inner(code.as_ref()).await
    }

    async fn send_2fa_request(client: Client, request_body: auth::v4::tfa::Post, log_message: &str) -> Result<Client> {
        info!("{}", log_message);
        let req = POST!("/auth/v4/2fa").body_json(request_body)?;
        let res = req.send_with(&client).await?;
        let res: auth::v4::tfa::PostRes = res.ok()?.into_body_json()?;

        info!(scopes = ?res.scopes, "building new auth object");
        let auth = (client.stores.get_auth())
            .map(|(_, auth)| auth.with_scopes(res.scopes))
            .await;

        if let Some(auth) = auth {
            info!(%auth, "storing new auth object");
            client.stores.set_auth(auth).await;
        } else {
            warn!("no auth object found");
            return Err(FlowErr::from(AuthStateErr).into());
        }

        Ok(client)
    }

    async fn totp_inner(self, code: &str) -> Result<Client> {
        let Self { client, .. } = self;
        let request_body = auth_tfa_post_totp_req(code);
        let log_message = format!("sending TOTP request with code: {}", code);
        
        Self::send_2fa_request(client, request_body, &log_message).await
    }
}

fn auth_info_post_req(username: &str) -> auth::v4::info::Post {
    auth::v4::info::Post {
        username: username.to_owned(),
    }
}

fn gen_proofs(pass: &str, info: &auth::v4::info::PostRes) -> Result<SRPProofB64, SRPError> {
    SRPAuth::new(
        &RPGPVerifier::default(),
        pass,
        info.version,
        &info.salt,
        &info.modulus,
        &info.server_ephemeral,
    )
    .and_then(|auth| auth.generate_proofs())
    .map(Into::into)
}

fn auth_post_req(
    username: &str,
    session: &str,
    proofs: &SRPProofB64,
    fingerprint: Option<Fingerprint>
) -> auth::v4::Post {
    auth::v4::Post {
        username: username.to_owned(),
        client_ephemeral: proofs.client_ephemeral.clone(),
        client_proof: proofs.client_proof.clone(),
        session: session.to_owned(),
        fingerprint: fingerprint.map(|f| { f.0 }).to_owned(),
    }
}

fn auth_tfa_post_totp_req(totp: &str) -> auth::v4::tfa::Post {
    auth::v4::tfa::Post::TOTP(totp.to_owned())
}

fn auth_tfa_post_fido2_req(fido_data: fido2::Request) -> auth::v4::tfa::Post {
    auth::v4::tfa::Post::FIDO(Box::new(fido_data))
}
