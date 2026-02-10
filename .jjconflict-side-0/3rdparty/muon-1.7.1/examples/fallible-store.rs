//! This example demonstrates the basic login flow for a muon client.
use anyhow::{bail, Result};
use async_trait::async_trait;
use muon::client::flow::LoginFlow;
use muon::client::Auth;
use muon::env::EnvId;
use muon::store::*;
use muon::{App, Client, GET};
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread;
use tracing::*;

/// The types of errors that can come out of my [`FallibleFileStore`]
#[derive(Debug, Clone, Copy)]
enum FallibleFileStoreErrors {
    /// The disk is full
    Full,
    /// The file is not found
    NotFound,
    /// The file is corrupted/not interpretable
    Corrupted,
    /// Something else
    #[allow(dead_code)]
    Other(std::io::ErrorKind),
}

impl From<&std::io::Error> for FallibleFileStoreErrors {
    fn from(value: &std::io::Error) -> Self {
        match value.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound,
            std::io::ErrorKind::WriteZero => Self::Full,
            kind => Self::Other(kind),
        }
    }
}

impl From<&serde_json::Error> for FallibleFileStoreErrors {
    fn from(_: &serde_json::Error) -> Self {
        Self::Corrupted
    }
}

/// Something that can handle errors and do something with it
/// Here we use some mpsc sender to send the error to other threads that might
/// be itnerested (in practice it would be an Observer pattern probably)
#[derive(Debug, Default, Clone)]
struct StoreErrorHandler(Vec<Sender<FallibleFileStoreErrors>>);

impl StoreErrorHandler {
    /// Deal with my concrete error type.
    /// Note: it is important that we can match the type to ensure everything is
    /// treated correctly.
    pub fn handle_error(&self, err: FallibleFileStoreErrors) {
        // e.g., write the error in a file
        let _ = tempfile::tempfile().and_then(|mut f| writeln!(f, "{:?}", err));
        // or send to other threads for asynchronous handling (e.g., display a modal)
        for notifiees in self.0.iter() {
            let _ = notifiees.send(err);
        }
    }
}

/// A store that persist in a file and that can fail
#[derive(Debug, Clone)]
struct FallibleFileStore {
    env: EnvId,
    dir: Arc<tempfile::TempDir>,
    err_handler: StoreErrorHandler,
}

impl FallibleFileStore {
    /// Create a prod file storage, it mostly create the initial file with
    /// Auth::none in it
    pub async fn prod(err_handler: StoreErrorHandler) -> std::io::Result<Self> {
        let dir = tempfile::tempdir()?;
        info!("Persistence storage in {:?}", dir.path());
        let path = dir.path().join("auth");
        let _ = File::options()
            .write(true)
            .truncate(true)
            .create(true)
            .open(path)?;
        let mut store = Self {
            env: EnvId::new_atlas(),
            dir: dir.into(),
            err_handler,
        };
        let _ = store.set_auth(Auth::None).await;
        Ok(store)
    }

    pub fn auth_file_path(&self) -> impl AsRef<Path> {
        self.dir.path().join("auth")
    }
}

#[async_trait]
impl Store for FallibleFileStore {
    fn env(&self) -> EnvId {
        self.env.clone()
    }

    async fn get_auth(&self) -> Auth {
        debug!("getting auth from {:?}", self.auth_file_path().as_ref());
        // Try to read the file, if an error occurs ask the handler to deal with it
        let Ok(auth_content) = std::fs::read(self.auth_file_path())
            .inspect_err(|e| self.err_handler.handle_error(e.into()))
        else {
            // otherwise, return the default Auth (unlogged)
            return Default::default();
        };
        debug!("auth {:?}", std::str::from_utf8(&auth_content));
        // try to interpret the file, if impossible, ask the error handler again... and
        // return the default session...
        let auth = serde_json::from_slice::<Auth>(&auth_content).unwrap_or_else(|e| {
            self.err_handler.handle_error((&e).into());
            Default::default()
        });
        debug!("auth {:?}", auth);
        // all good return the session in the file
        auth
    }

    async fn set_auth(&mut self, auth: Auth) -> Result<Auth, StoreError> {
        // try to open the file in write mode... it musts exist though! otherwise return
        // that we can't and ask  the hadnler to deal with the error
        let Ok(mut file) = File::options()
            .write(true)
            .truncate(true)
            .open(self.auth_file_path())
            .inspect_err(|e| self.err_handler.handle_error(e.into()))
        else {
            return Err(StoreError);
        };
        // This is not expected to fail but still in case we can ask the handler...
        let Ok(auth_str) = serde_json::to_string_pretty(&auth)
            .inspect_err(|e| self.err_handler.handle_error(e.into()))
        else {
            return Err(StoreError);
        };
        // write and in case of error propagate to handler...
        let _ = file
            .write_all(auth_str.as_bytes())
            .inspect_err(|e| self.err_handler.handle_error(e.into()));

        Ok(auth)
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let (gui_error_handler, error_handler_channel) = channel::<FallibleFileStoreErrors>();
    // this is our thread that will do the user output, it can be some GUI display,
    // CLI display, whatever... Herein, it is in a thread to mimick what would
    // happen in a real case: a thread doing the display
    let spawn = thread::spawn(move || {
        let err = error_handler_channel.recv().unwrap();

        match err {
            FallibleFileStoreErrors::Full => {
                log::error!("Persistent storage is full! Persistence is disabled")
            }
            FallibleFileStoreErrors::NotFound => {
                log::error!("Persistence storage file is absent! Persistence is disabled")
            }
            FallibleFileStoreErrors::Corrupted => {
                log::error!("Persistent storage is corrupted! Persistence is disabled")
            }
            FallibleFileStoreErrors::Other(_) => log::error!(
                "An unknown error happened on the persistence storage! Persistence is disabled"
            ),
        }
    });

    // First, define which app is using the client.
    let app = App::new("windows-vpn@4.1.0")?;

    // Set the user agent, if desired.
    let app = app.with_user_agent("Mozilla/5.0");

    // Then, specify where the client will persist its session data. We'll use the
    // TestStore for this example; a real app would implement its own store.
    // A store is tied to a specific environment; a prod store holds prod tokens,
    // an atlas store holds atlas tokens, etc.
    let store_error_handler = StoreErrorHandler(vec![gui_error_handler]);
    let store = FallibleFileStore::prod(store_error_handler).await?;

    // get a new connected client: this should work as the store is empty for now...
    // Finally, create the client. The client will be configured to connect to the
    // prod environment, and the session data will be stored in the TestStore.
    // Please check the auth-info-provider.rs example to see how to pass a
    // fingerprint to the muon client. The fingerprint is important in combating
    // fraud.
    let client = Client::new(app.clone(), store.clone())?;

    // Auth stuff is done via the auth flow.
    // To begin, call the auth method on the client.
    let auth = client.auth();

    // We can use the auth flow to login.
    let client = match auth.login("visionary", "a").await {
        // The client is now authenticated,
        // and the tokens are in the store.
        LoginFlow::Ok(client, _) => client,

        // The client needs 2FA to complete the login.
        // We can inspect the client to see what kind of 2FA is available.
        LoginFlow::TwoFactor(flow, _) => {
            if flow.has_totp() {
                flow.totp("123456").await?
            } else if flow.fido_details().is_some() {
                unimplemented!()
            } else {
                bail!("no 2FA available");
            }
        }
        LoginFlow::Failed { reason, .. } => return Err(reason.into()),
    };

    // Now we can use the client to make authenticated requests.
    // The client will automatically use the tokens in the store.
    // If the tokens are expired, the client will refresh them.
    info!("{}", client.send(GET!("/core/v4/users")).await?.ok()?);
    drop(client);

    // get a new client: we shouldn't try to refresh any token as we are already
    // logged, we can already make auth calls
    let client = Client::new(app.clone(), store.clone())?;
    info!(
        "with persistent storage: {}",
        client.send(GET!("/core/v4/users")).await?.ok()?
    );
    let _ = std::fs::remove_file(store.auth_file_path());
    // but we can still use the auth routes as we have an in memory store...
    info!(
        "without persistent storage: {}",
        client.send(GET!("/core/v4/users")).await?.ok()?
    );
    drop(client);
    // now that the persistent storage is gone, ...
    let client = Client::new(app.clone(), store.clone())?;
    // this will fail
    let err = client.send(GET!("/core/v4/users")).await?.ok();
    info!("without persistent storage: {err:?}");
    assert!(err.is_err());

    let _ = spawn.join();
    Ok(())
}
