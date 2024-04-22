mod delete_conversations;
mod event_loop;
mod label_conversations;
mod mark_conversations_read;
mod mark_conversations_unread;
mod move_conversations;
mod unlabel_conversations;

use crate::MailUserContext;
pub use delete_conversations::*;
pub use event_loop::*;
pub use label_conversations::*;
pub use mark_conversations_read::*;
pub use mark_conversations_unread::*;
pub use move_conversations::*;
use proton_action_queue::ActionFactory;
use std::sync::Weak;
pub use unlabel_conversations::*;

pub(crate) fn new_action_factory(mail_user_context: Weak<MailUserContext>) -> ActionFactory {
    let mut factory = ActionFactory::new();
    const ERR_MSG: &str = "Double Factory registration";
    factory
        .register(Box::new(DeleteConversationsActionFactory::new()))
        .expect(ERR_MSG);

    factory
        .register(Box::new(MarkConversationsReadActionFactory::new()))
        .expect(ERR_MSG);
    factory
        .register(Box::new(MarkConversationsUnreadActionFactory::new()))
        .expect(ERR_MSG);
    factory
        .register(Box::new(LabelConversationsActionFactory::new()))
        .expect(ERR_MSG);
    factory
        .register(Box::new(UnlabelConversationsActionFactory::new()))
        .expect(ERR_MSG);
    factory
        .register(Box::new(MoveConversationsActionFactory::new()))
        .expect(ERR_MSG);

    factory
        .register(Box::new(EventLoopActionFactory::new(mail_user_context)))
        .expect(ERR_MSG);

    factory
}
