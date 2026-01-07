use crate::MailContextError;
use crate::datatypes::{ConversationLabelsCount, MessageLabelsCount};
use crate::models::{Conversation, Message};
use anyhow::Context;
use itertools::Itertools;
use proton_core_api::services::proton::{AddressId, LabelId, ProtonCore};
use proton_core_api::session::Session;
use proton_core_common::models::{Address, Label, ModelIdExtension};
use proton_mail_api::services::proton::response_data::{
    Conversation as ApiConversation, MessageMetadata as ApiMessageMetadata,
};
use stash::orm::Model;
use stash::stash::{RunTransaction, StashError, Tether};
use std::collections::{BTreeMap, HashSet};
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
            let missing_labels = self.fetch_missing_labels(api, tx).await?;

            tx.run_tx(async |tx| {
                Label::store_labels_async(tx, missing_labels.clone())
                    .await
                    .context("Failed to store missing labels")?;
                let message_counts = missing_labels
                    .iter()
                    .map(|v| MessageLabelsCount {
                        label_id: v.remote_id.clone().unwrap(),
                        total: 0,
                        unread: 0,
                    })
                    .collect::<Vec<_>>();
                let conversation_counts = missing_labels
                    .iter()
                    .map(|v| ConversationLabelsCount {
                        label_id: v.remote_id.clone().unwrap(),
                        total: 0,
                        unread: 0,
                    })
                    .collect::<Vec<_>>();
                MessageLabelsCount::upsert(message_counts, tx).await?;
                ConversationLabelsCount::upsert(conversation_counts, tx).await?;
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
        if self.label_is_missing(label_id, tether).await? {
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

    async fn fetch_missing_labels(
        &self,
        api: &Session,
        tx: &mut impl RunTransaction,
    ) -> Result<Vec<Label>, MailContextError> {
        info!("Syncing missing labels: {:?}", self.label_ids);
        let mut missing_labels =
            Label::get_labels_by_ids(api, self.label_ids.iter().cloned().collect()).await?;
        let parent_label_ids = missing_labels
            .iter()
            .filter_map(|l| l.remote_parent_id.clone())
            .collect_vec();

        if !parent_label_ids.is_empty() {
            tracing::info!("Missing labels have parents: {:?}", parent_label_ids);

            let tether = tx.tether();
            let missing_parents_ids = self.find_missing_labels(parent_label_ids, tether).await?;

            if !missing_parents_ids.is_empty() {
                tracing::debug!(
                    "Detected missing parent labels among fetched dependencies: {:?}",
                    missing_parents_ids
                );
                let missing_parents = Label::get_labels_by_ids(api, missing_parents_ids).await?;
                let grandparent_label_ids = missing_parents
                    .iter()
                    .filter_map(|l| l.remote_parent_id.clone())
                    .collect_vec();
                let missing_ancestry = self
                    .find_missing_labels(grandparent_label_ids, tether)
                    .await?;

                if missing_ancestry.is_empty() {
                    missing_labels.extend(missing_parents);
                } else {
                    let selected_types = missing_labels
                        .iter()
                        .filter_map(|l| {
                            l.remote_parent_id.as_ref()?;
                            Some(l.label_type)
                        })
                        .collect_vec();
                    tracing::info!(
                        "Detected missing label's ancestry lineage, fetching by types instead: {:?}",
                        selected_types
                    );
                    let all_labels_in_selected_types =
                        Label::fetch_labels(api, &selected_types).await?;
                    let label_ids = all_labels_in_selected_types
                        .iter()
                        .filter_map(|l| l.remote_id.clone())
                        .collect_vec();
                    let missing_ancestry_ids = self.find_missing_labels(label_ids, tether).await?;
                    let missing_ancestry = all_labels_in_selected_types
                        .into_iter()
                        .filter(|al| missing_ancestry_ids.contains(al.remote_id.as_ref().unwrap()));
                    // BTree was chosen to preserve API ordering,
                    // this is needed for setting `local_parent_id` field correctly.
                    let mut uniques: BTreeMap<LabelId, Label> = BTreeMap::new();
                    for label in missing_ancestry.chain(missing_labels) {
                        uniques
                            .entry(label.remote_id.clone().unwrap())
                            .or_insert(label);
                    }
                    missing_labels = uniques.into_values().collect();
                    tracing::debug!(
                        "Established missing labels: {:?}",
                        missing_labels
                            .iter()
                            .filter_map(|l| l.remote_id.as_ref())
                            .collect_vec()
                    )
                }
            }
        }

        Ok(missing_labels)
    }

    async fn find_missing_labels(
        &self,
        label_ids: impl IntoIterator<Item = LabelId>,
        tether: &Tether,
    ) -> Result<Vec<LabelId>, StashError> {
        let mut missing = vec![];

        for label_id in label_ids {
            if self.label_is_missing(&label_id, tether).await? {
                missing.push(label_id)
            }
        }

        Ok(missing)
    }

    async fn label_is_missing(
        &self,
        label_id: &LabelId,
        tether: &Tether,
    ) -> Result<bool, StashError> {
        Ok(Label::remote_id_counterpart(label_id.clone(), tether)
            .await?
            .is_none())
    }
}
