use crate::event_loop::event_subscriber::CoreEventSubscriberError;
use crate::models::{Address, UserSettings};
use futures::StreamExt;
use futures::stream::FuturesOrdered;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{Action, AddressId, CoreEventV6, ProtonCore, User};
use proton_core_api::session::Session;
use proton_event_loop::v6::EventSource;
use std::collections::HashMap;

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
        if event.users.as_ref().is_some_and(|v| !v.is_empty()) {
            self.get_or_fetch_user(session).await?;
        }

        if event.user_settings.as_ref().is_some_and(|v| !v.is_empty()) {
            self.get_or_fetch_user_settings(session).await?;
        }

        if let Some(events) = &event.addresses {
            self.fetch_addresses(
                session,
                events
                    .iter()
                    .filter_map(|e| (e.action != Action::Delete).then_some(e.id.clone())),
            )
            .await?;
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

    pub async fn fetch_addresses(
        &mut self,
        session: &Session,
        id: impl IntoIterator<Item = AddressId>,
    ) -> Result<(), ApiServiceError> {
        let mut tasks = id
            .into_iter()
            .filter(|id| self.addresses.contains_key(id))
            .map(|id| async { (id.clone(), session.get_address_by_id(id).await) })
            .collect::<FuturesOrdered<_>>();
        while let Some((id, task)) = tasks.next().await {
            let response = task.inspect_err(|e| tracing::error!("Failed to fetch {id:?}: {e}"))?;
            self.addresses.insert(id, response.address.into());
        }
        Ok(())
    }

    pub fn get_address_mut(&mut self, id: &AddressId) -> Option<&mut Address> {
        self.addresses.get_mut(id)
    }
}
