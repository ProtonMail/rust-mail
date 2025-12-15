use proton_core_common::{
    datatypes::LocalLabelId,
    models::{Label, LabelError, ModelExtension},
};
use stash::{
    macros::Model,
    orm::Model,
    stash::{Bond, Tether},
};
use tracing::{info, instrument};

pub trait LabelExt {
    fn is_idle(&self, tether: &Tether) -> impl Future<Output = Result<bool, LabelError>> + Send;
    fn is_busy(&self, tether: &Tether) -> impl Future<Output = Result<bool, LabelError>> + Send;

    fn as_busy(
        &self,
        tether: &Tether,
    ) -> impl Future<Output = Result<Option<MailBusyLabel>, LabelError>>;

    fn mark_busy(&self, bond: &Bond<'_>) -> impl Future<Output = Result<(), LabelError>>;
    fn mark_idle(&self, bond: &Bond<'_>) -> impl Future<Output = Result<(), LabelError>>;
}

impl LabelExt for Label {
    #[instrument(skip_all)]
    async fn is_idle(&self, tether: &Tether) -> Result<bool, LabelError> {
        Ok(self.as_busy(tether).await?.is_none())
    }

    #[instrument(skip_all)]
    async fn is_busy(&self, tether: &Tether) -> Result<bool, LabelError> {
        Ok(self.as_busy(tether).await?.is_some())
    }

    #[instrument(skip_all)]
    async fn as_busy(&self, tether: &Tether) -> Result<Option<MailBusyLabel>, LabelError> {
        Ok(MailBusyLabel::load(self.id(), tether).await?)
    }

    #[instrument(skip_all)]
    async fn mark_busy(&self, bond: &Bond<'_>) -> Result<(), LabelError> {
        let id = self.id();

        info!(?id, "Marking label as busy");

        MailBusyLabel { id }.save(bond).await?;

        Ok(())
    }

    #[instrument(skip_all)]
    async fn mark_idle(&self, bond: &Bond<'_>) -> Result<(), LabelError> {
        info!(id = ?self.id(), "Marking label as idle");

        if let Some(busy) = self.as_busy(bond).await? {
            busy.delete(bond).await?;
        }

        Ok(())
    }
}

/// Keeps track of labels for which a `delete all` action is running.
///
/// Basically, if this record exists, then label is currently being emptied -
/// this record gets created when you schedule a `delete all` action and later
/// gets removed once we get a confirmation from the server that the action has
/// completed.
///
/// See scroller's tests for more details - look for tests related to the
/// "delete all" functionality.
#[derive(Clone, Debug, Eq, Model, PartialEq)]
#[TableName("mail_busy_labels")]
pub struct MailBusyLabel {
    #[IdField]
    pub id: LocalLabelId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::test_context::MailTestContext;
    use proton_core_api::services::proton::Label as ApiLabel;
    use proton_core_common::datatypes::SystemLabel;

    fn api_label(id: &str, parent_id: Option<&str>) -> ApiLabel {
        ApiLabel {
            id: id.into(),
            parent_id: parent_id.map(Into::into),
            ..ApiLabel::test_default()
        }
    }

    async fn system_label(tether: &mut Tether, label: SystemLabel) -> Label {
        let mut label = Label::from(api_label(&label.remote_id(), None));

        tether
            .tx(async |bond| label.save(bond).await)
            .await
            .unwrap();

        label
    }

    #[tokio::test]
    async fn busy() {
        let ctx = MailTestContext::new().await;
        let mut tether = ctx.user_context().await.stash().connection().await.unwrap();

        let inbox = system_label(&mut tether, SystemLabel::Inbox).await;
        let trash = system_label(&mut tether, SystemLabel::Trash).await;

        // ---

        assert!(inbox.is_idle(&tether).await.unwrap());
        assert!(!inbox.is_busy(&tether).await.unwrap());

        assert!(trash.is_idle(&tether).await.unwrap());
        assert!(!trash.is_busy(&tether).await.unwrap());

        // ---

        tether
            .tx(async |bond| trash.mark_busy(bond).await)
            .await
            .unwrap();

        assert!(inbox.is_idle(&tether).await.unwrap());
        assert!(!inbox.is_busy(&tether).await.unwrap());

        assert!(!trash.is_idle(&tether).await.unwrap());
        assert!(trash.is_busy(&tether).await.unwrap());

        // ---

        tether
            .tx(async |bond| {
                inbox.mark_idle(bond).await.unwrap();
                trash.mark_idle(bond).await
            })
            .await
            .unwrap();

        assert!(inbox.is_idle(&tether).await.unwrap());
        assert!(!inbox.is_busy(&tether).await.unwrap());

        assert!(trash.is_idle(&tether).await.unwrap());
        assert!(!trash.is_busy(&tether).await.unwrap());
    }
}
