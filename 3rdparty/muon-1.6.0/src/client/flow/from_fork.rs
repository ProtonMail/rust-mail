use crate::auth::Auth;
use crate::client::Client;
use crate::flow::{FlowErr, UserIdErr};
use crate::http::{Status, GET};
use crate::rest::auth;
use crate::util::ByteSliceExt;
use crate::Tokens;

/// A flow for acquiring a fork.
#[must_use]
#[derive(Debug)]
pub struct FromForkFlow {
    client: Client,
}

impl FromForkFlow {
    pub(super) fn new(client: Client) -> Self {
        Self { client }
    }

    /// Acquire the fork with the given selector.
    ///
    /// The selector is a unique identifier for the fork.
    pub async fn with_selector(self, selector: impl AsRef<str>) -> WithSelectorFlow {
        WithSelectorFlow::new(self.client, selector.as_ref()).await
    }

    /// Acquire the fork from a user code.
    pub async fn with_code(self) -> WithCodeFlow {
        WithCodeFlow::new(self.client).await
    }
}

/// A flow for a fork with a selector.
#[must_use]
#[derive(Debug)]
pub enum WithSelectorFlow {
    /// The flow is complete; the client is returned, along with any payload.
    Ok(Client, Option<Vec<u8>>),
    /// The login via selector couldn't proceed
    Failed {
        /// The child originating the login via selector
        client: Client,
        /// The reason why it did not succeed
        reason: FlowErr,
    },
}

impl WithSelectorFlow {
    async fn new(client: Client, selector: &str) -> Self {
        let Client { stores, .. } = &client;

        info!(%selector, "acquiring forked session");
        let req = GET!("/auth/v4/sessions/forks/{selector}");
        let res = return_variant_on_error!(client.send(req).await, client, Self::Failed);
        let res = return_variant_on_error!(res.ok(), client, Self::Failed);
        let res: auth::v4::sessions::forks::GetIdRes =
            return_variant_on_error!(res.into_body_json(), client, Self::Failed);

        // Build new access tokens from the fork.
        let tok = Tokens::access(
            res.auth.access_token,
            res.auth.refresh_token,
            res.auth.scopes,
        );

        // Get the ID of the user that logged in.
        let Some(user_id) = res.auth.user_id else {
            return Self::Failed {
                client,
                reason: UserIdErr.into(),
            };
        };

        info!(uid = %res.auth.uid, "building new auth object");
        let auth = Auth::internal(user_id, res.auth.uid, tok);

        info!(%auth, "auth complete, storing tokens");
        stores.set_auth(auth).await;

        let payload = return_variant_on_error!(
            (res.payload).map(|p| p.b64_into()).transpose(),
            client,
            Self::Failed
        );

        Self::Ok(client, payload)
    }
}

/// A flow for a fork with a user code.
#[must_use]
#[derive(Debug)]
pub enum WithCodeFlow {
    /// Poll the fork for completion.
    Poll(WithCodePollFlow),

    /// The flow is complete; the client is returned.
    Ok(Client, Option<Vec<u8>>),

    /// The flow couldn't complete; the client is returned unlogged
    Failed {
        /// The client that failed to log via a code flow
        client: Client,
        /// The reason why this login method failed
        reason: FlowErr,
    },
}

impl WithCodeFlow {
    async fn new(client: Client) -> Self {
        // Generate a new random human-readable code.
        let req = GET!("/auth/v4/sessions/forks");
        let res = return_variant_on_error!(client.send(req).await, client, Self::Failed);
        let res = return_variant_on_error!(res.ok(), client, Self::Failed);
        let res: auth::v4::sessions::forks::GetRes =
            return_variant_on_error!(res.into_body_json(), client, Self::Failed);

        // The selector should be used to poll, the code returned to the user.
        let selector = res.selector;
        let code = res.code;

        Self::Poll(WithCodePollFlow {
            client,
            selector,
            code,
        })
    }
}

/// A flow for polling a fork with a user code.
#[must_use]
#[derive(Debug)]
pub struct WithCodePollFlow {
    client: Client,
    selector: String,
    code: String,
}

impl WithCodePollFlow {
    /// Return the user code used for the fork process
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Poll the fork for completion.
    pub async fn poll(self) -> WithCodeFlow {
        let Client { stores, .. } = &self.client;

        info!(selector = %self.selector, "polling forked session");
        let req = GET!("/auth/v4/sessions/forks/{}", self.selector);
        let res = return_variant_on_error!(
            self.client.send(req).await,
            self.client,
            WithCodeFlow::Failed
        );

        // If the fork is not yet complete, return a poll flow.
        let res: auth::v4::sessions::forks::GetIdRes = if res.is(Status::UNPROCESSABLE_ENTITY) {
            return WithCodeFlow::Poll(self);
        } else {
            return_variant_on_error!(res.into_body_json(), self.client, WithCodeFlow::Failed)
        };

        // Build new access tokens from the fork.
        let tok = Tokens::access(
            res.auth.access_token,
            res.auth.refresh_token,
            res.auth.scopes,
        );

        // Get the ID of the user that logged in.
        let Some(user_id) = res.auth.user_id else {
            return WithCodeFlow::Failed {
                client: self.client,
                reason: UserIdErr.into(),
            };
        };

        info!(uid = %res.auth.uid, "building new auth object");
        let auth = Auth::internal(user_id, res.auth.uid, tok);

        info!(%auth, "fork complete, storing tokens");
        stores.set_auth(auth).await;

        let payload = return_variant_on_error!(
            (res.payload).map(|p| p.b64_into()).transpose(),
            self.client,
            WithCodeFlow::Failed
        );

        WithCodeFlow::Ok(self.client, payload)
    }
}
