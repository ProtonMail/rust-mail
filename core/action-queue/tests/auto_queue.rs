mod common;

use crate::common::DefaultError;
use common::{new_queue_typed, new_session};
use proton_action_queue::action::{Action, DefaultVersionConverter, Handler, Type};
use proton_action_queue::queue::ActionRemoteOutput;
use proton_api_core::session::Session;
use serde::{Deserialize, Serialize};
use stash::stash::{Stash, Tether};

#[tokio::test]
async fn auto_queued_on_network_failure() {
    // check if the remote action returns a network error it is queued for execution later.
    let session = new_session().await;
    let queue = new_queue_typed::<ErrorAction>().await;

    for error in [
        ErrorType::Timeout,
        ErrorType::Connect,
        ErrorType::Redirect,
        ErrorType::Http429,
        ErrorType::Http500,
        ErrorType::Http503,
    ] {
        let output = queue
            .apply_action(&session, ErrorAction { error_type: error })
            .await
            .unwrap();

        assert!(
            matches!(output.remote, ActionRemoteOutput::Queued(_)),
            "Error type {error:?} did not result in queued action"
        );
    }
}

#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
enum ErrorType {
    Timeout,
    Redirect,
    Connect,
    Http429,
    Http500,
    Http503,
}
#[derive(Serialize, Deserialize)]
struct ErrorAction {
    error_type: ErrorType,
}

impl Action for ErrorAction {
    const TYPE: Type = Type("error");
    const VERSION: u32 = 1;
    type VersionConverter = DefaultVersionConverter<Self>;
    type Handler = ErrorActionHandler;
    type RemoteOutput = u32;

    type LocalOutput = ();

    type Error = DefaultError;

    type Context = ();
}

#[derive(Default)]
struct ErrorActionHandler {}

impl Handler for ErrorActionHandler {
    type Action = ErrorAction;

    type Context = ();

    async fn apply_local(
        &self,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        // Nothing to do
        Ok(())
    }

    async fn revert_local(
        &self,
        _: &Self::Context,
        _: &mut Self::Action,
        _: &Tether,
    ) -> Result<(), <Self::Action as Action>::Error> {
        panic!("should not be called");
    }

    async fn apply_remote(
        &self,
        _: &Self::Context,
        action: &mut Self::Action,
        _: &Session,
        _: &Stash,
    ) -> Result<<Self::Action as Action>::RemoteOutput, <Self::Action as Action>::Error> {
        use proton_api_core::service::ApiServiceError;
        let err = match action.error_type {
            ErrorType::Timeout => ApiServiceError::Timeout(String::new()),
            ErrorType::Redirect => ApiServiceError::Redirect(String::new(), String::new()),
            ErrorType::Connect => ApiServiceError::ConnectionError(String::new()),
            ErrorType::Http429 => ApiServiceError::TooManyRequest(String::new(), String::new()),
            ErrorType::Http500 => {
                ApiServiceError::InternalServerError(String::new(), String::new())
            }
            ErrorType::Http503 => ApiServiceError::ServiceUnavailable(String::new(), String::new()),
        };

        Err(err.into())
    }
}
