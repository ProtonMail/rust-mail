#[cfg(test)]
#[path = "./tests/device_registration.rs"]
mod tests;

use std::{collections::HashSet, sync::Arc};

use futures::StreamExt;
use itertools::Itertools;
use proton_core_api::{
    service::ApiServiceError,
    services::proton::{ProtonCore, SessionId, muon::Status, prelude::RegisterDeviceRequest},
};
use stash::{
    exports::ToSql,
    orm::Model,
    stash::{StashError, Tether, WatcherHandle},
};
use tokio::{sync::watch, task::JoinHandle};

use crate::{
    Context, CoreContextError,
    datatypes::{RegisteredDevice, StoredDevicePrivateKey, StoredDevicePublicKey},
    db::account::CoreSession,
    models::ModelExtension,
};

/// How long we should sleep just in case there was a network error other than offline issue.
const SLEEP_IN_CASE_OF_NETWORK_ERR: u64 = 500;

/// Error code encountered when session has missing scopes.
/// In the context of device registation this usually means "You are logged in, but still waiting for 2FA".
const MISSING_SCOPES_ERROR_CODE: u32 = 9100;

/// Spawns a background task that is responsible for registering devices for push notifications.
/// It automatically detects whenever a new session is created.
///
/// # Parameters
///
/// * `device_rx` - stream of device registration details. If changed it must contain `Some`
///
#[allow(clippy::result_large_err)]
#[tracing::instrument(err, skip_all)]
pub async fn spawn_registered_device_task(
    ctx: Arc<Context>,
    device_rx: watch::Receiver<Option<RegisteredDevice>>,
) -> Result<JoinHandle<()>, RegisteredDeviceTaskError> {
    let sessions_watcher = CoreSession::watch(ctx.account_stash()).await?;
    let ctx_clone = ctx.clone();
    let handle = ctx.spawn(async move {
        if let Err(e) = registered_device_task(ctx_clone, sessions_watcher, device_rx).await {
            tracing::error!("Registering device tokens task failed {e:?}");
        }
    });
    Ok(handle)
}

#[derive(Debug, thiserror::Error)]
pub enum RegisteredDeviceTaskError {
    #[error("Could not create a user context from session")]
    CreateContext(#[from] CoreContextError),

    #[error(transparent)]
    Stash(#[from] StashError),

    #[error("Stream receiving device tokens from client has failed: {0}")]
    DeviceStream(#[from] watch::error::RecvError),

    #[error("Stream watching core sessions has ended prematurely")]
    SessionStreamEnded,

    #[error("Failed to generate device key pair")]
    Crypto,

    #[error("API error: {0}")]
    API(#[from] ApiServiceError),
}

impl RegisteredDeviceTaskError {
    // Whether we should repeat the step if it failed
    //
    fn is_network_failure(&self) -> bool {
        match self {
            RegisteredDeviceTaskError::API(api_service_error) => {
                api_service_error.is_network_failure()
            }
            _ => false,
        }
    }

    // Checks whether error comes from the fact, that the session is not yet fully
    // authenticated (for example waiting for 2FA)
    //
    fn is_not_fully_authenticated(&self) -> bool {
        match self {
            RegisteredDeviceTaskError::API(ApiServiceError::OtherHttpError(
                Status::FORBIDDEN,
                _,
                api_error_info,
            )) => {
                api_error_info.as_ref().map(|a| a.code).unwrap_or_default()
                    == MISSING_SCOPES_ERROR_CODE
            }
            _ => false,
        }
    }
}

/// Internal state of a background task responsible for registering devices
/// for push notifications.
/// It keeps track of which session was already registered and what was the last device
/// token.
///
// Public: For the sake of re-exporting and breaking dependency cycle in our tests.
#[derive(Default)]
pub struct RegisteredDeviceTaskState {
    device: Option<RegisteredDevice>,
    registered_sessions: HashSet<SessionId>,
}

#[tracing::instrument(err, skip_all)]
async fn registered_device_task(
    ctx: Arc<Context>,
    sessions_watcher: WatcherHandle,
    mut device_rx: watch::Receiver<Option<RegisteredDevice>>,
) -> Result<(), RegisteredDeviceTaskError> {
    let mut sessions_stream = sessions_watcher.receiver.into_stream();
    let mut state = RegisteredDeviceTaskState::default();

    loop {
        registered_device_task_step(&ctx, &mut state, &mut sessions_stream, &mut device_rx).await?;
    }
}

// This function is public because we have to re-import it in tests via proton_core_test_utils
// in order to break dependency cycle.
/// One step of the background task that registers device.
///
#[tracing::instrument(err, skip_all)]
pub async fn registered_device_task_step(
    ctx: &Context,
    state: &mut RegisteredDeviceTaskState,
    sessions_stream: &mut flume::r#async::RecvStream<'_, ()>,
    device_rx: &mut watch::Receiver<Option<RegisteredDevice>>,
) -> Result<(), RegisteredDeviceTaskError> {
    let sessions = tokio::select! {
        res = device_rx.changed() => {
            tracing::debug!("Device details changed: {res:?}");
            res?;

            state.device.clone_from(&device_rx.borrow_and_update());
            // New device token registered. We need to re-register all sessions.
            state.registered_sessions.clear();
            let tether = ctx.account_stash().connection().await?;
            CoreSession::all(&tether).await?
        },
        res = sessions_stream.next() => {
            tracing::debug!("Sessions changed: {res:?}");
            res.ok_or(RegisteredDeviceTaskError::SessionStreamEnded)?;

            let tether = ctx.account_stash().connection().await?;
            // New session has been created. Instead of re-registering everything, we only
            // process unregistered sessions.
            get_unregistered_sessions(&tether, &state.registered_sessions).await?
        }
    };

    if sessions.is_empty() {
        return Ok(());
    }

    let mut network_status_observer = ctx.network_monitor_service().os_network_status_observer();

    if let Some(device) = state.device.as_ref() {
        // Trying in a loop. If registration fails because of network, let's retry.
        loop {
            if let Err(e) = register_sessions(
                ctx,
                sessions.clone(),
                &mut state.registered_sessions,
                device.clone(),
            )
            .await
            {
                if e.is_network_failure() {
                    tracing::error!("Network failure, waiting for online...");
                    // Recoverable failure. Repeat.
                    // Most likely happened because of network issue, so let's
                    // see if we are online.

                    network_status_observer.wait_until_online().await;

                    tracing::trace!("Device is online... Sleeping just in case");
                    // Even though we just waited for online, we should still sleep for a while.
                    // It is, because if the endpoint returns 500, the /ping endpoint may actually
                    // work just fine, so the `is_online` would return true, and yet we would
                    // spam the broken endpoint without any timeout.
                    tokio::time::sleep(tokio::time::Duration::from_millis(
                        SLEEP_IN_CASE_OF_NETWORK_ERR,
                    ))
                    .await;
                    tracing::trace!("Awake. Retrying");
                    continue;
                }
                if e.is_not_fully_authenticated() {
                    // Session is not fully authenticated. We are waiting for 2FA to finish.
                    // When it finishes, it will trigger session update with new scopes,
                    // therefore instead of repeating that registration, let's skip it for now.
                    break;
                }

                return Err(e);
            }
            break;
        }
    }
    Ok(())
}

/// Returns sessions that were not already registered.
///
#[allow(trivial_casts)]
async fn get_unregistered_sessions(
    tether: &Tether,
    registered_sessions: &HashSet<SessionId>,
) -> Result<Vec<CoreSession>, StashError> {
    let params = registered_sessions
        .iter()
        .cloned()
        .map(|v| Box::new(v) as Box<dyn ToSql + Send>)
        .collect_vec();
    CoreSession::find(
        format!(
            "WHERE remote_id NOT IN ({})",
            stash::utils::placeholders_n(params.len())
        ),
        params,
        tether,
    )
    .await
}

async fn register_sessions(
    ctx: &Context,
    sessions: Vec<CoreSession>,
    registered_sessions: &mut HashSet<SessionId>,
    device: RegisteredDevice,
) -> Result<(), RegisteredDeviceTaskError> {
    tracing::debug!("Registering sessions: {}", sessions.len());
    for session in sessions {
        register_session(ctx, session, registered_sessions, &device).await?;
    }
    tracing::debug!("Registered successfully");
    Ok(())
}

#[tracing::instrument(err, skip_all, fields(session_id = ?session.remote_id))]
async fn register_session(
    ctx: &Context,
    session: CoreSession,
    registered_sessions: &mut HashSet<SessionId>,
    device: &RegisteredDevice,
) -> Result<(), RegisteredDeviceTaskError> {
    let session_ctx = ctx.user_context_from_session(&session).await?;

    let pgp = proton_crypto::new_pgp_provider();

    let private_key = ctx
        .load_secret::<StoredDevicePrivateKey>()
        .map_err(|_| RegisteredDeviceTaskError::Crypto)?;

    let public_key = match private_key {
        None => ctx
            .gen_device_key_pair(&pgp)
            .map_err(|_| RegisteredDeviceTaskError::Crypto)?,

        Some(key) => {
            let device_key = key
                .to_device_key(&pgp)
                .map_err(|_| RegisteredDeviceTaskError::Crypto)?;

            let public_key = device_key
                .export_public_key(&pgp)
                .map_err(|_| RegisteredDeviceTaskError::Crypto)?;

            StoredDevicePublicKey::from(public_key)
        }
    };

    session_ctx
        .session()
        .register_device(RegisterDeviceRequest {
            device_token: device.device_token.clone(),
            environment: device.environment.into(),
            public_key: Some(public_key.to_string()),
            ping_notification_status: device.ping_notification_status,
            push_notification_status: device.push_notification_status,
        })
        .await?;

    registered_sessions.insert(session.remote_id.clone());

    Ok(())
}
