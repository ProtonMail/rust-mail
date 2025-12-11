use crate::event_loop::event_subscriber::CoreEventSubscriberError;
use crate::models::{Contact, Label};
use futures::StreamExt;
use futures::stream::FuturesOrdered;
use itertools::Itertools;
use proton_core_api::service::ApiServiceError;
use proton_core_api::services::proton::{
    Action, ContactId, ContactRootEventV6, LabelId, ProtonCore,
};
use proton_core_api::session::Session;
use proton_event_loop::v6::EventSource;
use std::collections::HashMap;

pub struct ContactEventSourceV6;

impl EventSource for ContactEventSourceV6 {
    type Event = ContactRootEventV6;
    type Cache = ContactEventCache;

    fn name() -> &'static str {
        "contacts-v6"
    }
}

#[derive(Default)]
pub struct ContactEventCache {
    contacts: HashMap<ContactId, Contact>,
    labels: HashMap<LabelId, Label>,
}

impl ContactEventCache {
    pub async fn fetch_event_data(
        &mut self,
        event: &ContactRootEventV6,
        session: &Session,
    ) -> Result<(), CoreEventSubscriberError> {
        if let Some(events) = &event.contacts {
            let mut contact_ids = Vec::with_capacity(events.len());
            for event in events {
                if event.action != Action::Delete && event.action != Action::UpdateFlags {
                    contact_ids.push(event.id.clone());
                }
            }

            self.fetch_contacts(session, contact_ids).await?;
        }

        if let Some(events) = &event.labels {
            let mut label_ids = Vec::with_capacity(events.len());
            for event in events {
                if event.action != Action::Delete {
                    label_ids.push(event.id.clone());
                }
            }
            self.fetch_labels(session, label_ids).await?;
        }

        Ok(())
    }
    pub async fn fetch_contacts(
        &mut self,
        session: &Session,
        id: impl IntoIterator<Item = ContactId>,
    ) -> Result<(), ApiServiceError> {
        let mut tasks = id
            .into_iter()
            .filter(|id| !self.contacts.contains_key(id))
            .map(|id| async { (id.clone(), session.get_contact(id).await) })
            .collect::<FuturesOrdered<_>>();
        while let Some((id, task)) = tasks.next().await {
            let response =
                task.inspect_err(|e| tracing::error!("Failed to fetch contact {id:?}: {e}"))?;
            self.contacts.insert(id, response.contact.into());
        }
        Ok(())
    }

    pub fn get_contact_mut(&mut self, id: &ContactId) -> Option<&mut Contact> {
        self.contacts.get_mut(id)
    }

    pub async fn fetch_labels(
        &mut self,
        session: &Session,
        id: impl IntoIterator<Item = LabelId>,
    ) -> Result<(), ApiServiceError> {
        const MAX_CURRENT_LABEL_REQUEST: usize = 50;
        let mut tasks = id
            .into_iter()
            .filter(|id| !self.labels.contains_key(id))
            .chunks(MAX_CURRENT_LABEL_REQUEST)
            .into_iter()
            .map(|ids| session.get_labels_by_ids(ids.collect()))
            .collect::<FuturesOrdered<_>>();
        while let Some(task) = tasks.next().await {
            let response = task.inspect_err(|e| tracing::error!("Failed to fetch labels: {e}"))?;
            for label in response.labels {
                self.labels.insert(label.id.clone(), label.into());
            }
        }
        Ok(())
    }

    pub fn get_label_mut(&mut self, id: &LabelId) -> Option<&mut Label> {
        self.labels.get_mut(id)
    }
}
