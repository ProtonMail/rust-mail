use crate::app::{AppBackgroundDispatcher, AppLocalDispatcher};
use crate::events::login::LoginEvent;
use crate::events::mailbox::MailboxEvent;
use crate::events::AppEvent;
use crate::state::AppState;
use crate::views::TotpView;
use proton_async::runtime::MultiThreaded;
use proton_mail_common::proton_api_mail::proton_api_core::exports::tracing::{debug, error};
use proton_mail_common::proton_api_mail::proton_api_core::login::Flow;
use proton_mail_common::{MailContext, MailContextResult};
use secrecy::{ExposeSecret, SecretString};

pub enum LoginState {
    LoggedOut,
    LoggingIn,
    AwaitingTotp(Flow),
    SubmittingTotp,
}

#[derive(Debug, thiserror::Error)]
pub enum LoginStateError {
    #[error("Database Error: {0}")]
    DB(#[from] proton_mail_common::db::DBError),
    #[error("Migration Error: {0}")]
    DBMigration(#[from] proton_mail_common::db::DBMigrationError),
    #[error("Network Error: {0}")]
    Http(#[from] proton_mail_common::proton_api_mail::proton_api_core::http::RequestError),
    #[error("Invalid Login State")]
    InvalidState,
}

impl LoginState {
    fn login(
        &mut self,
        mail_context: &MailContext,
        dispatcher: AppBackgroundDispatcher<AppState, AppEvent>,
        email: String,
        password: SecretString,
    ) -> MailContextResult<()> {
        self.logout(mail_context.async_runtime());
        *self = LoginState::LoggingIn;
        let mut login_flow = mail_context.new_login_flow(None)?;
        mail_context.async_runtime().spawn(async move {
            if let Err(e) = login_flow
                .login(&email, password.expose_secret(), None)
                .await
            {
                dispatcher
                    .queue_event_async(LoginEvent::LoginFailed(e))
                    .await;
                return;
            }

            if login_flow.is_awaiting_2fa() {
                dispatcher
                    .queue_event_async(LoginEvent::LoginNeed2FA(login_flow))
                    .await;
                return;
            }

            dispatcher
                .queue_event_async(LoginEvent::LoginSuccess(login_flow))
                .await;
        });
        Ok(())
    }

    fn submit_2fa(
        &mut self,
        dispatcher: AppBackgroundDispatcher<AppState, AppEvent>,
        runtime: &MultiThreaded,
        code: String,
    ) {
        let state = std::mem::replace(self, LoginState::SubmittingTotp);
        if let LoginState::AwaitingTotp(mut flow) = state {
            runtime.spawn(async move {
                match flow.submit_totp(&code).await {
                    Ok(_) => {
                        dispatcher
                            .queue_event_async(LoginEvent::LoginSuccess2FA(flow))
                            .await;
                    }
                    Err(e) => {
                        dispatcher
                            .queue_event_async(LoginEvent::Login2FAFailed((flow, e)))
                            .await;
                    }
                };
            });
        } else {
            *self = state;
            dispatcher.set_error("Invalid Login State", LoginStateError::InvalidState);
        }
    }
    fn logout(&mut self, runtime: &MultiThreaded) {
        match std::mem::replace(self, LoginState::LoggedOut) {
            LoginState::AwaitingTotp(flow) => {
                debug!("Logging out from TOTP state");
                runtime.spawn(async move {
                    if let Err(e) = flow.session().logout().await {
                        error!("Failed to logout :{e}");
                    }
                });
            }
            _ => {
                debug!("No state found for logout");
            }
        }
    }

    fn session_from_login_flow(
        &mut self,
        mut dispatcher: AppLocalDispatcher<AppState, AppEvent>,
        flow: Flow,
        mail_context: &MailContext,
    ) {
        match mail_context.user_context_from_login_flow(&flow) {
            Ok(ctx) => {
                dispatcher.queue_event(MailboxEvent::NewMailboxSession(ctx));
            }
            Err(e) => {
                dispatcher.set_error("Context Error", e);
            }
        }
        *self = LoginState::LoggedOut;
    }

    pub fn handle_event(
        &mut self,
        mut dispatcher: AppLocalDispatcher<AppState, AppEvent>,
        mail_context: &MailContext,
        event: LoginEvent,
    ) {
        match event {
            LoginEvent::LoginRequest { user, password } => {
                if let Err(e) = self.login(
                    mail_context,
                    dispatcher.background_dispatcher(),
                    user,
                    password,
                ) {
                    *self = LoginState::LoggedOut;
                    dispatcher.set_error("Failed to Login", e);
                }
            }
            LoginEvent::TwoFARequest(code) => {
                self.submit_2fa(
                    dispatcher.background_dispatcher(),
                    mail_context.async_runtime(),
                    code,
                );
            }
            LoginEvent::LoginFailed(err) => {
                *self = LoginState::LoggedOut;
                dispatcher.set_error("Failed to Login", err);
            }
            LoginEvent::LoginSuccess2FA(flow) => {
                dispatcher.pop_view(); // pop 2fa
                dispatcher.pop_view(); // pop login
                self.session_from_login_flow(dispatcher, flow, mail_context)
            }
            LoginEvent::LoginSuccess(flow) => {
                dispatcher.pop_view();
                self.session_from_login_flow(dispatcher, flow, mail_context)
            }
            LoginEvent::LoginNeed2FA(flow) => {
                *self = LoginState::AwaitingTotp(flow);
                dispatcher.push_view(TotpView::new())
            }
            LoginEvent::Login2FAFailed((flow, error)) => {
                *self = LoginState::AwaitingTotp(flow);
                dispatcher.set_error("Failed to Submit Two Factor Code", error);
            }
            LoginEvent::Logout => {
                self.logout(mail_context.async_runtime());
            }
        }
    }
}
