use proton_api_rs::domain::{Label, LabelId, LabelType};
use proton_api_rs::http::Client;
use proton_api_rs::{http, Session};
use proton_async::async_trait::async_trait;

#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait Provider {
    async fn get_labels(&self, label_type: LabelType) -> http::Result<Vec<Label>>;

    async fn create_label<'a>(
        &self,
        name: &str,
        color: &str,
        label_type: LabelType,
        parent_id: Option<&'a LabelId>,
    ) -> http::Result<Label>;

    async fn update_label<'a>(
        &self,
        id: &LabelId,
        name: &str,
        color: &str,
        parent_id: Option<&'a LabelId>,
    ) -> http::Result<Label>;

    async fn delete_label(&self, id: &LabelId) -> http::Result<()>;
}

pub struct ProtonProvider {
    client: Client,
    session: Session,
}

impl ProtonProvider {
    pub fn new(client: Client, session: Session) -> Self {
        Self { client, session }
    }
}

#[async_trait]
impl Provider for ProtonProvider {
    async fn get_labels(&self, label_type: LabelType) -> http::Result<Vec<Label>> {
        self.session.get_labels(&self.client, label_type).await
    }

    async fn create_label<'a>(
        &self,
        name: &str,
        color: &str,
        label_type: LabelType,
        parent_id: Option<&'a LabelId>,
    ) -> http::Result<Label> {
        self.session
            .create_label(&self.client, name, color, label_type, parent_id)
            .await
    }

    async fn update_label<'a>(
        &self,
        id: &LabelId,
        name: &str,
        color: &str,
        parent_id: Option<&'a LabelId>,
    ) -> http::Result<Label> {
        self.session
            .update_label(&self.client, id, name, color, parent_id)
            .await
    }

    async fn delete_label(&self, id: &LabelId) -> http::Result<()> {
        self.session.delete_label(&self.client, id).await
    }
}
