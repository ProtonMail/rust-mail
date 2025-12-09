use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{CoreEvent, ProtonCore, User};
use proton_core_api::session::Session;
use proton_event_loop::v6::EventSource;

pub struct CoreEventSource;

impl EventSource for CoreEventSource {
    type Event = CoreEvent;
    type Cache = CoreEventCache;

    fn name() -> &'static str {
        "core"
    }
}

#[derive(Default)]
pub struct CoreEventCache {
    user: Option<User>,
}

impl CoreEventCache {
    pub async fn get_or_fetch_user(&mut self, api: &Session) -> Result<&User, ApiServiceError> {
        let user = &mut self.user;

        if let Some(user) = user {
            Ok(user)
        } else {
            Ok(user.insert(api.get_users().await?.user))
        }
    }

    pub fn set_user(&mut self, user: User) {
        self.user = Some(user);
    }
}
