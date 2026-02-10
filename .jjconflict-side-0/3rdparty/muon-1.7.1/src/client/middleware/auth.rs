use crate::auth::{Auth, Tokens};
use crate::client::headers::{AuthTokenHeader, AuthUidHeader};
use crate::client::ClientInternalStorage;
use crate::common::{BoxFut, Sender, SenderLayer};
use crate::http::{HttpReq, HttpReqExt, HttpRes, Status, StatusErr, POST};
use crate::rest::auth;
use crate::store::{AuthVersion, SafeStore, StoreWriteGuard};
use crate::{Error, ErrorKind, InfoProvider, Result};
use futures::TryFutureExt;
use http::status::StatusCode;
use muon_proc::autoimpl;
use std::borrow::Borrow;
use std::sync::Arc;
use thiserror::Error;

/// Errors that can occur while using the auth layer.
#[derive(Debug, Error)]
pub enum AuthErr {
    /// The session could not be refreshed.
    #[error("refresh failed")]
    Refresh,

    /// The session does not exist.
    #[error("non-existent session")]
    Session,
}

/// An auth layer: sets the auth headers from the store if available,
/// refreshes the auth session if necessary, and retries the request.
#[must_use]
#[derive(Debug)]
pub struct AuthLayer {
    stores: ClientInternalStorage,
    provider: Option<Arc<dyn InfoProvider>>,
}

impl AuthLayer {
    /// Create a new auth layer from the given store.
    pub fn new(stores: ClientInternalStorage) -> Self {
        AuthLayer {
            stores,
            provider: None,
        }
    }

    /// Add an info provider to the auth layer. Used to ask for the fingerprint.
    pub fn with_info_provider(mut self, provider: Arc<dyn InfoProvider>) -> Self {
        self.provider = Some(provider);
        self
    }
}

impl AuthLayer {
    async fn on_send<S>(&self, inner: &S, req: HttpReq) -> Result<HttpRes, AuthLayerErr>
    where
        S: Sender<HttpReq, HttpRes>,
        S: ?Sized,
    {
        let (ver, auth) = self.stores.get_auth().await;

        let (ver, auth) = if let Auth::None = auth {
            info!("attempting to get unauth session");
            let res = new_unauth_session(inner, self.stores.local(), &ver, self.provider.clone())
                .await?;

            info!("create unauth session succeeded, sync to persistent store");
            self.stores.sync_stores().await;

            res
        } else {
            trace!("using existing session");
            (ver, auth)
        };

        let Some(uid) = auth.uid() else {
            trace!("no auth session available, sending as-is");
            return Ok(inner.send(req).await?);
        };

        trace!(%uid, "attaching auth UID to request");
        let req = req.header(AuthUidHeader::new(uid));

        let Some(tok) = auth.tokens() else {
            trace!(%uid, "no auth tokens available, sending as-is");
            return Ok(inner.send(req).await?);
        };

        if let Some(tok) = tok.acc_tok() {
            trace!(%uid, "attaching auth token to request");
            let req = req.clone().header(AuthTokenHeader::new(tok));

            debug!(%uid, "sending authenticated request");
            match inner.send(req).await? {
                t if !t.is(Status::UNAUTHORIZED) => return Ok(t),
                _ => warn!(%uid, "session unauthorized"),
            }
        }

        match refresh_store(inner, self.stores.local(), &ver, self.provider.clone()).await {
            Ok(auth) => match auth.acc_tok() {
                Some(tok) => {
                    info!(%uid, "refresh succeeded, sync to persistent store");
                    self.stores.sync_stores().await;

                    info!(%uid, "sending authenticated request with updated tokens");
                    Ok(inner.send(req.header(AuthTokenHeader::new(tok))).await?)
                }

                None => {
                    error!(%uid, "no access token available after refresh");
                    Err(AuthErr::Refresh)?
                }
            },

            Err(err @ AuthLayerErr::Auth(AuthErr::Session)) => {
                warn!(%uid, "auth session no longer exists");
                self.stores.local().set_auth(Auth::None).await;
                self.stores.sync_stores().await;

                Err(err)
            }

            Err(err) => Err(err),
        }
    }
}

impl SenderLayer<HttpReq, HttpRes> for AuthLayer {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<HttpReq, HttpRes>,
        req: HttpReq,
    ) -> BoxFut<'a, Result<HttpRes>> {
        Box::pin(self.on_send(inner, req).err_into())
    }
}

async fn auth_refresh<S>(inner: &S, uid: &str, tok: &str) -> Result<Tokens, AuthLayerErr>
where
    S: Sender<HttpReq, HttpRes>,
    S: ?Sized,
{
    // Build the refresh request.
    let req = auth::v4::refresh::Post {
        refresh_token: tok.to_owned(),
        response_type: "token".to_owned(),
        grant_type: "refresh_token".to_owned(),
        redirect_uri: "https://protonmail.ch".to_owned(),
    };

    // Send the refresh request.
    let res = POST!("/auth/v4/refresh")
        .header(AuthUidHeader::new(uid))
        .body_json(req)?
        .send_with(inner)
        .await?;

    // Parse the refresh response.
    let res: auth::v4::refresh::PostRes = match res.ok() {
        Ok(res) => {
            trace!("auth refresh successful");
            res.into_body_json()?
        }

        Err(err) => {
            error!(%err, "unexpected error during auth refresh");
            return Err(AuthLayerErr::StatusErr(err));
        }
    };

    // The UID should *never* change.
    assert_eq!(res.auth.uid, uid);

    // Build the new set of tokens.
    Ok(Tokens::access(
        res.auth.access_token,
        res.auth.refresh_token,
        res.auth.scopes,
    ))
}

async fn refresh_store<S>(
    inner: &S,
    store: &SafeStore,
    ver: &AuthVersion,
    provider: Option<Arc<dyn InfoProvider>>,
) -> Result<Auth, AuthLayerErr>
where
    S: Sender<HttpReq, HttpRes>,
    S: ?Sized,
{
    trace!("locking store for auth refresh");
    let mut store = store.write().await;

    trace!("checking current auth session version");
    match store.get_auth().await {
        (ref cur, auth) if cur == ver => {
            if let Auth::Anonymous { uid, tok } = auth {
                refresh_unauthenticated_session(inner, &mut store, &uid, &tok, provider).await
            } else if let Auth::Internal { user_id, uid, tok } = auth {
                refresh_authenticated_session(inner, &mut store, &user_id, &uid, &tok).await
            } else {
                Ok(Auth::None)
            }
        }

        (_, auth) => {
            trace!("auth session already updated");
            Ok(auth)
        }
    }
}

async fn refresh_authenticated_session<S>(
    inner: &S,
    store: &mut StoreWriteGuard<'_>,
    user_id: &str,
    uid: &str,
    tok: &Tokens,
) -> Result<Auth, AuthLayerErr>
where
    S: Sender<HttpReq, HttpRes>,
    S: ?Sized,
{
    info!(%uid, "attempting auth refresh");
    let tok = match auth_refresh(inner, uid, tok.ref_tok()).await {
        Ok(tok) => tok,

        Err(AuthLayerErr::StatusErr(StatusErr(code, _))) if code.requires_logout() => {
            if code == StatusCode::BAD_REQUEST {
                error!("non-existent auth session. Received a 400 from the backend. This indicates a problem with the refresh logic. It could be that the same token was refreshed twice. Please check!");
            } else {
                error!("non-existent auth session");
            }

            return Err(AuthErr::Session)?;
        }

        Err(err) => {
            error!(%err, "unexpected error during auth refresh");
            return Err(AuthErr::Refresh)?;
        }
    };

    info!(%uid, "building new auth object");
    let auth = Auth::internal(user_id, uid, tok);

    info!("storing new auth tokens");
    store.set_auth(auth.clone()).await;

    Ok(auth)
}

async fn refresh_unauthenticated_session<S>(
    inner: &S,
    store: &mut StoreWriteGuard<'_>,
    uid: &str,
    tok: &Tokens,
    provider: Option<Arc<dyn InfoProvider>>,
) -> Result<Auth, AuthLayerErr>
where
    S: Sender<HttpReq, HttpRes>,
    S: ?Sized,
{
    info!(%uid, "attempting unauth session refresh");
    match auth_refresh(inner, uid, tok.ref_tok()).await {
        Ok(tok) => {
            info!(%uid, "building new unauth session object");
            let auth = Auth::anonymous(uid, tok);

            info!("storing new unauth session tokens");
            store.set_auth(auth.clone()).await;

            Ok(auth)
        }

        Err(AuthLayerErr::StatusErr(StatusErr(code, _))) if code.requires_unauth() => {
            info!(%uid, "refresh failed - creating a new unauth session object");
            let auth = match unauth_session(inner, provider).await {
                Ok(auth) => auth,
                Err(err) => {
                    error!(%err, "unexpected error during unauth session create");
                    return Err(AuthErr::Session)?;
                }
            };

            info!("storing new unauth session tokens");
            store.set_auth(auth.clone()).await;

            Ok(auth)
        }

        Err(err) => {
            error!(%err, "unexpected error during unauth session refresh");
            Err(AuthErr::Refresh)?
        }
    }
}

async fn unauth_session<S>(
    inner: &S,
    provider: Option<Arc<dyn InfoProvider>>,
) -> Result<Auth, AuthLayerErr>
where
    S: Sender<HttpReq, HttpRes>,
    S: ?Sized,
{
    let mut req = POST!("/auth/v4/sessions");
    if let Some(reference) = &provider {
        if let Some(fingerprint) = reference.fingerprint().await {
            req = req.body(fingerprint.0.to_string());
        }
    }

    // Send the unauth session request
    let res = req.send_with(inner).await?;

    // Parse the unauth session response.
    let res: auth::v4::Auth = match res.ok() {
        Ok(res) => {
            trace!("unauth session created successfully");
            res.into_body_json()?
        }

        Err(err) => {
            error!(%err, "unexpected error during unauth session request");
            return Err(AuthLayerErr::StatusErr(err));
        }
    };

    // Build the new set of tokens.
    let tok = Tokens::access(res.access_token, res.refresh_token, res.scopes);

    info!("building new auth object");
    let auth = Auth::anonymous(res.uid, tok);

    Ok(auth)
}

async fn new_unauth_session<S>(
    inner: &S,
    store: &SafeStore,
    ver: &AuthVersion,
    provider: Option<Arc<dyn InfoProvider>>,
) -> Result<(AuthVersion, Auth), AuthLayerErr>
where
    S: Sender<HttpReq, HttpRes>,
    S: ?Sized,
{
    trace!("locking store to save unauth session credentials");
    let mut store = store.write().await;

    trace!("checking current auth session version");
    match store.get_auth().await {
        (ref cur, _) if cur == ver => {
            info!("requesting unauth session");
            let auth = unauth_session(inner, provider).await?;

            info!("storing new auth tokens");
            let new_ver: AuthVersion = store.set_auth(auth.clone()).await;

            Ok((new_ver, auth))
        }

        (ver, auth) => {
            trace!("unauth session already updated");
            Ok((ver, auth))
        }
    }
}

mod errors {
    use super::*;

    #[derive(Debug, Error)]
    #[error("invalid store state")]
    pub struct StoreStateErr;

    #[derive(Debug, Error)]
    #[error("auth layer: {0}")]
    pub enum AuthLayerErr {
        Auth(#[from] AuthErr),
        StatusErr(#[from] StatusErr),
        StoreState(#[from] StoreStateErr),
        Inner(#[from] Error),
    }

    impl From<AuthLayerErr> for Error {
        fn from(err: AuthLayerErr) -> Self {
            if let AuthLayerErr::Inner(err) = err {
                err.map_kind(ErrorKind::Send)
            } else {
                ErrorKind::send(err)
            }
        }
    }
}

use self::errors::*;

#[autoimpl]
trait StatusCodeExt: Borrow<StatusCode> {
    fn requires_logout(&self) -> bool {
        [
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::UNPROCESSABLE_ENTITY,
        ]
        .contains(self.borrow())
    }

    fn requires_unauth(&self) -> bool {
        [
            StatusCode::BAD_REQUEST,
            StatusCode::UNAUTHORIZED,
            StatusCode::MISDIRECTED_REQUEST,
            StatusCode::UNPROCESSABLE_ENTITY,
        ]
        .contains(self.borrow())
    }
}
