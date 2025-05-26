use crate::Mailbox;
use proton_core_common::models::Label;
use stash::orm::Model;
use stash::stash::Tether;

#[allow(async_fn_in_trait)]
pub trait MailboxTestUtils {
    async fn get_local_label(&self, stash: &Tether) -> Label;
}

impl MailboxTestUtils for Mailbox {
    async fn get_local_label(&self, tether: &Tether) -> Label {
        Label::load(self.label_id(), tether).await.unwrap().unwrap()
    }
}
