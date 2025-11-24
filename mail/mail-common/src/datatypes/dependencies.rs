use crate::MailContextError;
use crate::datatypes::{ConversationLabelsCount, MessageLabelsCount};
use crate::models::{Conversation, Message};
use anyhow::Context;
use proton_core_api::services::proton::{AddressId, LabelId, ProtonCore};
use proton_core_api::session::Session;
use proton_core_common::models::{Address, Label, ModelIdExtension};
use proton_mail_api::services::proton::response_data::{
    Conversation as ApiConversation, MessageMetadata as ApiMessageMetadata,
};
use stash::orm::Model;
use stash::stash::{RunTransaction, StashError, Tether};
use std::collections::HashSet;
use tracing::info;

#[derive(Default)]
pub struct MessageOrConversationDependencyFetcher {
    label_ids: HashSet<LabelId>,
    address_ids: HashSet<AddressId>,
}

impl MessageOrConversationDependencyFetcher {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn check_api_conversation(
        &mut self,
        conv: &ApiConversation,
        tether: &Tether,
    ) -> Result<(), StashError> {
        for label_id in conv.labels.iter().map(|l| &l.id) {
            self.check_label_id(label_id, tether).await?;
        }
        Ok(())
    }

    pub async fn check_conversation(
        &mut self,
        conv: &Conversation,
        tether: &Tether,
    ) -> Result<(), StashError> {
        for label_id in conv
            .labels
            .iter()
            .filter_map(|l| l.remote_label_id.as_ref())
        {
            self.check_label_id(label_id, tether).await?;
        }
        Ok(())
    }

    pub async fn check_api_message_metadata(
        &mut self,
        message: &ApiMessageMetadata,
        tether: &Tether,
    ) -> Result<(), StashError> {
        self.check_label_ids(message.label_ids.iter().cloned(), tether)
            .await?;

        self.check_address_id(&message.address_id, tether).await?;

        Ok(())
    }

    pub async fn check_message(
        &mut self,
        message: &Message,
        tether: &Tether,
    ) -> Result<(), StashError> {
        self.check_label_ids(message.label_ids.iter().cloned(), tether)
            .await?;
        self.check_address_id(&message.remote_address_id, tether)
            .await?;

        Ok(())
    }

    pub async fn fetch_and_store(
        &self,
        api: &Session,
        tx: &mut impl RunTransaction,
    ) -> Result<(), MailContextError> {
        if !self.label_ids.is_empty() {
            info!("Syncing missing labels: {:?}", self.label_ids);
            let missing_labels =
                Label::get_labels_by_ids(api, self.label_ids.iter().cloned().collect()).await?;
            tx.run_tx(async |tx| {
                Label::store_labels_async(tx, missing_labels)
                    .await
                    .context("Failed to store missing labels")?;
                // Create missing counters
                let message_counts = self
                    .label_ids
                    .iter()
                    .map(|v| MessageLabelsCount {
                        label_id: v.clone(),
                        total: 0,
                        unread: 0,
                    })
                    .collect::<Vec<_>>();
                let conversation_counts = self
                    .label_ids
                    .iter()
                    .map(|v| ConversationLabelsCount {
                        label_id: v.clone(),
                        total: 0,
                        unread: 0,
                    })
                    .collect::<Vec<_>>();
                MessageLabelsCount::create_or_update_message_counts(message_counts, tx).await?;
                ConversationLabelsCount::create_or_update_conversation_counts(
                    conversation_counts,
                    tx,
                )
                .await?;
                Ok(())
            })
            .await
            .map_err(MailContextError::Other)?;
        }

        if !self.address_ids.is_empty() {
            let mut addresses = Vec::with_capacity(self.address_ids.len());
            info!("Syncing missing addresses: {:?}", self.address_ids);
            for address_id in &self.address_ids {
                let address = api.get_address_by_id(address_id.clone()).await?.address;
                addresses.push(Address::from(address));
            }
            tx.run_tx(async |tx| {
                for mut address in addresses {
                    address.save(tx).await?;
                }
                Ok(())
            })
            .await
            .map_err(MailContextError::Other)?;
        }

        Ok(())
    }

    pub async fn check_label_ids(
        &mut self,
        label_ids: impl IntoIterator<Item = LabelId>,
        tether: &Tether,
    ) -> Result<(), StashError> {
        for label_id in label_ids {
            self.check_label_id(&label_id, tether).await?;
        }
        Ok(())
    }

    pub async fn check_label_id(
        &mut self,
        label_id: &LabelId,
        tether: &Tether,
    ) -> Result<(), StashError> {
        if Label::remote_id_counterpart(label_id.clone(), tether)
            .await?
            .is_none()
        {
            self.label_ids.insert(label_id.clone());
        }
        Ok(())
    }

    pub async fn check_address_id(
        &mut self,
        address_id: &AddressId,
        tether: &Tether,
    ) -> Result<(), StashError> {
        if Address::remote_id_counterpart(address_id.clone(), tether)
            .await?
            .is_none()
        {
            self.address_ids.insert(address_id.clone());
        }
        Ok(())
    }
}
