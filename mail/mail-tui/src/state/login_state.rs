use crate::app::AppDispatcher;
use crate::events::AppEvents;
use crate::state::{AppState, DataLoadError, UserState};
use anyhow::anyhow;
use log::error;
use proton_api_mail::proton_api_core;
use proton_api_mail::proton_api_core::http::HttpRequestError;
use proton_api_mail::proton_api_core::{LoginError, Session, SessionType, TotpSession};
use proton_async::runtime::MTRuntime;
use secrecy::{ExposeSecret, SecretString};
use std::path::PathBuf;

pub enum LoginState {
    LoggedOut,
    AwaitingTotp(TotpSession),
    LoggedIn(UserState),
}

impl LoginState {
    pub fn login(
        &mut self,
        dispatcher: AppDispatcher<AppState, AppEvents>,
        runtime: &MTRuntime,
        db_path: PathBuf,
        email: String,
        password: SecretString,
    ) {
        self.logout(runtime);
        runtime.spawn(async move {
            match login(&email, password.expose_secret()).await {
                Ok(s) => match s {
                    SessionType::Authenticated(s) => {
                        let user_state = UserState::new(s, db_path).await;
                        dispatcher
                            .queue_event_async(AppEvents::login_success(user_state))
                            .await;
                    }
                    SessionType::AwaitingTotp(s) => {
                        dispatcher
                            .queue_event_async(AppEvents::login_needs_2fa(s))
                            .await;
                    }
                },
                Err(e) => {
                    dispatcher
                        .queue_event_async(AppEvents::login_failed(e))
                        .await;
                }
            }
        });
    }

    pub fn submit_2fa(
        &self,
        dispatcher: AppDispatcher<AppState, AppEvents>,
        runtime: &MTRuntime,
        db_path: PathBuf,
        code: String,
    ) {
        if let LoginState::AwaitingTotp(s) = self {
            let s = s.clone();
            runtime.spawn(async move {
                match s.submit_totp(&code).await {
                    Ok(s) => {
                        dispatcher
                            .queue_event_async(AppEvents::login_success(
                                UserState::new(s, db_path).await,
                            ))
                            .await;
                    }
                    Err(e) => {
                        dispatcher
                            .queue_event_async(AppEvents::login_2fa_failed(e))
                            .await;
                    }
                };
            });
        } else {
            dispatcher.set_error(
                "Invalid Login State",
                DataLoadError::Other(anyhow!("Not waiting on any 2FA calls")),
            );
        }
    }
    pub fn logout(&mut self, runtime: &MTRuntime) {
        match std::mem::replace(self, LoginState::LoggedOut) {
            LoginState::AwaitingTotp(t) => {
                runtime.spawn(async move {
                    if let Err(e) = t.logout().await {
                        error!("Failed to logout :{e}");
                    }
                });
            }
            LoginState::LoggedIn(user_state) => {
                runtime.spawn(async move {
                    if let Err(e) = user_state.session.session().logout().await {
                        error!("Failed to logout :{e}");
                    }
                });
            }
            _ => {
            }
        }
    }
}

async fn login(email: &str, password: &str) -> Result<SessionType, LoginError> {
    let client = proton_api_core::http::ClientBuilder::new()
        .app_version("Other")
        .build()
        .map_err(|e| LoginError::Request(HttpRequestError::Other(e)))?;
    Session::login(client, email, password, None).await
}
