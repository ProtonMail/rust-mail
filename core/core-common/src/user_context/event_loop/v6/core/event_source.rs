use crate::event_loop::event_subscriber::CoreEventSubscriberError;
use crate::models::{Address, UserSettings};
use futures::StreamExt;
use futures::future::BoxFuture;
use futures::stream::FuturesUnordered;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{Action, AddressId, CoreEventV6, ProtonCore, User};
use proton_core_api::session::Session;
use proton_event_loop::v6::EventSource;
use std::collections::HashMap;
use tracing::error;

pub struct CoreEventSourceV6;

impl EventSource for CoreEventSourceV6 {
    type Event = CoreEventV6;
    type Cache = CoreEventCache;

    fn name() -> &'static str {
        "core-v6"
    }
}

#[derive(Default)]
pub struct CoreEventCache {
    user: Option<User>,
    user_settings: Option<UserSettings>,
    addresses: HashMap<AddressId, Address>,
}

impl CoreEventCache {
    pub async fn fetch_event_data(
        &mut self,
        event: &CoreEventV6,
        session: &Session,
    ) -> Result<(), CoreEventSubscriberError> {
        let mut tasks = FuturesUnordered::<FutureTask>::new();

        if event.users.as_ref().is_some_and(|v| !v.is_empty()) {
            let session = session.clone();
            tasks.push(Box::pin(async move {
                session
                    .get_users()
                    .await
                    .inspect_err(|e| error!("Failed to get user: {e}"))
                    .map(|u| FetchData::User(u.user))
            }));
        }

        if event.user_settings.as_ref().is_some_and(|v| !v.is_empty()) {
            let session = session.clone();
            tasks.push(Box::pin(async move {
                session
                    .get_settings()
                    .await
                    .inspect_err(|e| error!("Failed to get user settings: {e}"))
                    .map(|u| FetchData::Settings(u.user_settings.into()))
            }));
        }

        if let Some(events) = &event.addresses {
            for id in events
                .iter()
                .filter_map(|e| (e.action != Action::Delete).then_some(e.id.clone()))
            {
                let session = session.clone();
                tasks.push(Box::pin(async move {
                    session
                        .get_address_by_id(id.clone())
                        .await
                        .inspect_err(|e| error!("Failed to get {id:?}: {e}"))
                        .map(|a| FetchData::Address(id, a.address.into()))
                }));
            }
        }

        let mut first_err = None;
        while let Some(result) = tasks.next().await {
            match result {
                Ok(data) => data.apply(self),
                Err(e) => {
                    // try to collect as man successful requests as possible
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
            }
        }

        if let Some(first_err) = first_err {
            return Err(first_err.into());
        }

        Ok(())
    }
    pub async fn get_or_fetch_user(&mut self, api: &Session) -> Result<&mut User, ApiServiceError> {
        let user = &mut self.user;

        if let Some(user) = user {
            Ok(user)
        } else {
            Ok(user.insert(
                api.get_users()
                    .await
                    .inspect_err(|e| tracing::error!("Failed to fetch user: {e}"))?
                    .user,
            ))
        }
    }

    pub fn set_user(&mut self, user: User) {
        self.user = Some(user);
    }

    pub fn get_user_mut(&mut self) -> Option<&mut User> {
        self.user.as_mut()
    }

    pub async fn get_or_fetch_user_settings(
        &mut self,
        api: &Session,
    ) -> Result<&mut UserSettings, ApiServiceError> {
        let user_settings = &mut self.user_settings;

        if let Some(settings) = user_settings {
            Ok(settings)
        } else {
            Ok(user_settings.insert(api.get_settings().await?.user_settings.into()))
        }
    }

    pub fn get_user_settings_mut(&mut self) -> Option<&mut UserSettings> {
        self.user_settings.as_mut()
    }

    pub fn get_address_mut(&mut self, id: &AddressId) -> Option<&mut Address> {
        self.addresses.get_mut(id)
    }
}

type FutureTask = BoxFuture<'static, Result<FetchData, ApiServiceError>>;
enum FetchData {
    Address(AddressId, Address),
    User(User),
    Settings(UserSettings),
}

impl FetchData {
    fn apply(self, cache: &mut CoreEventCache) {
        match self {
            FetchData::Address(id, address) => {
                cache.addresses.insert(id, address);
            }
            FetchData::User(user) => cache.user = Some(user),
            FetchData::Settings(settings) => {
                cache.user_settings = Some(settings);
            }
        }
    }
}
