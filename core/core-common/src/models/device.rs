#[cfg(test)]
#[path = "../tests/models/device.rs"]
mod tests;

use std::{collections::HashSet, sync::Arc};

use futures::StreamExt;
use itertools::Itertools;
use proton_api_core::{
    service::ApiServiceError,
    services::proton::{ProtonCore, SessionId, prelude::RegisterDeviceRequest},
    session::CoreSession as _,
};
use proton_task_service::AsyncTaskResult;
use stash::{
    exports::ToSql,
    macros::Model,
    orm::Model,
    stash::{StashError, Tether, WatcherHandle},
};
use tokio::{sync::watch, task::JoinHandle};

use crate::{
    Context, CoreContextError,
    datatypes::{DeviceEnvironment, StoredDevicePrivateKey, StoredDevicePublicKey},
    db::account::CoreSession,
    models::ModelExtension,
};

// TODO (wpolak): Remove this table and this structure
// We no longer need to store tokens in the database.
// We might want to keep only minimal datastructure in memory.
//
/// This model is used to registed the device for Push notifications.
///
/// Note, that in the database at the same time there should be only one row in `registered_devices`.
/// It is because there should be only one session for one app.
///
#[derive(Clone, Debug, Eq, PartialEq, Model)]
#[TableName("registered_devices")]
pub struct RegisteredDevice {
    /// Device token, used as primary key
    #[IdField]
    pub device_token: String,

    /// Environment to which we register
    #[DbField]
    pub environment: DeviceEnvironment,

    /// TODO: Document this field
    #[DbField]
    pub ping_notification_status: Option<i32>,

    /// TODO: Document this field
    #[DbField]
    pub push_notification_status: Option<i32>,

    /// The internal row ID of the record in the database. This is assigned by
    /// `SQLite`, and is used as a consistent identifier for records when
    /// listening for change notifications.
    #[RowIdField]
    pub row_id: Option<u64>,
}

/// Spawns a background task that is responsible for registering devices for push notifications.
/// It automatically detects whenever a new session is created.
///
/// # Parameters
///
/// * `ctx` - core context.
/// * `device_rx` - stream of device registration details. If changed it must contain `Some`
///
pub async fn spawn_registered_device_task(
    ctx: Arc<Context>,
    device_rx: watch::Receiver<Option<RegisteredDevice>>,
) -> Result<JoinHandle<AsyncTaskResult<()>>, RegisteredDeviceTaskError> {
    let sessions_watcher = CoreSession::watch(ctx.account_stash())?;
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
pub async fn registered_device_task_step(
    ctx: &Context,
    state: &mut RegisteredDeviceTaskState,
    sessions_stream: &mut flume::r#async::RecvStream<'_, ()>,
    device_rx: &mut watch::Receiver<Option<RegisteredDevice>>,
) -> Result<(), RegisteredDeviceTaskError> {
    let sessions = tokio::select! {
        res = device_rx.changed() => {
            res?;

            state.device.clone_from(&device_rx.borrow_and_update());
            // New device token registered. We need to re-register all sessions.
            state.registered_sessions.clear();
            let tether = ctx.account_stash().connection();
            CoreSession::all(&tether).await?
        },
        res = sessions_stream.next() => {
            res.ok_or(RegisteredDeviceTaskError::SessionStreamEnded)?;

            let tether = ctx.account_stash().connection();
            // New session has been created. Instead of re-registering everything, we only
            // process unregistered sessions.
            get_unregistered_sessions(&tether, &state.registered_sessions).await?
        }
    };
    if let Some(device) = state.device.as_ref() {
        register_sessions(
            ctx,
            sessions,
            &mut state.registered_sessions,
            device.clone(),
        )
        .await?;
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
            stash::utils::placeholders(params.len())
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
    for session in sessions {
        register_session(ctx, session, registered_sessions, &device).await?;
    }
    Ok(())
}

#[tracing::instrument(skip_all, fields(session_id = ?session.remote_id))]
async fn register_session(
    ctx: &Context,
    session: CoreSession,
    registered_sessions: &mut HashSet<SessionId>,
    device: &RegisteredDevice,
) -> Result<(), RegisteredDeviceTaskError> {
    let session_ctx = ctx.user_context_from_session(&session, None).await?;

    let pgp_provider = proton_crypto::new_pgp_provider();
    let private_key = ctx
        .load_secret::<StoredDevicePrivateKey>()
        .map_err(|_| RegisteredDeviceTaskError::Crypto)?;
    let public_key = match private_key {
        None => ctx
            .gen_device_key_pair(&pgp_provider)
            .map_err(|_| RegisteredDeviceTaskError::Crypto)?,
        Some(key) => {
            let device_key = key
                .to_device_key(&pgp_provider)
                .map_err(|_| RegisteredDeviceTaskError::Crypto)?;

            let public_key = device_key
                .export_public_key(&pgp_provider)
                .map_err(|_| RegisteredDeviceTaskError::Crypto)?;
            StoredDevicePublicKey::from(public_key)
        }
    };

    device
        .register(session_ctx.session().api(), public_key)
        .await?;

    registered_sessions.insert(session.remote_id.clone());
    Ok(())
}

impl RegisteredDevice {
    /// Registers the device for Push Notifications.
    ///
    /// # Errors
    ///
    /// Returns an error if the API call fails
    ///
    pub async fn register<API: ProtonCore>(
        &self,
        api: &API,
        public_key: StoredDevicePublicKey,
    ) -> Result<(), ApiServiceError> {
        api.register_device(RegisterDeviceRequest {
            device_token: self.device_token.clone(),
            environment: self.environment.into(),
            public_key: Some(public_key.to_string()),
            ping_notification_status: self.ping_notification_status,
            push_notification_status: self.push_notification_status,
        })
        .await?;
        Ok(())
    }
}
