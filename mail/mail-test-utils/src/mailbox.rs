use ::stash::stash::Stash;
use proton_mail_common::models::Label;
use proton_mail_common::Mailbox;
use stash::orm::Model;

#[allow(async_fn_in_trait)]
pub trait MailboxTestUtils {
    async fn get_local_label(&self, stash: &Stash) -> Label;
}

impl MailboxTestUtils for Mailbox {
    async fn get_local_label(&self, stash: &Stash) -> Label {
        Label::load(self.label_id(), stash).await.unwrap().unwrap()
    }
}
