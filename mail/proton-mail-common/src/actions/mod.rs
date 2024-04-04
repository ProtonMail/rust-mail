mod delete_conversations;
mod event_loop;
mod label_conversations;
mod mark_conversations_read;
mod mark_conversations_unread;
mod unlabel_conversations;

use crate::WeakMailUserContext;
pub use delete_conversations::*;
pub use event_loop::*;
pub use label_conversations::*;
pub use mark_conversations_read::*;
pub use mark_conversations_unread::*;
use proton_action_queue::ActionFactory;
pub use unlabel_conversations::*;

pub(crate) fn new_action_factory(mail_user_context: WeakMailUserContext) -> ActionFactory {
    let mut factory = ActionFactory::new();
    const ERR_MSG: &str = "Double Factory registration";
    factory
        .register(Box::new(DeleteConversationsActionFactory::new(
            mail_user_context.clone(),
        )))
        .expect(ERR_MSG);

    factory
        .register(Box::new(MarkConversationsReadActionFactory::new(
            mail_user_context.clone(),
        )))
        .expect(ERR_MSG);
    factory
        .register(Box::new(MarkConversationsUnreadActionFactory::new(
            mail_user_context.clone(),
        )))
        .expect(ERR_MSG);
    factory
        .register(Box::new(LabelConversationsActionFactory::new(
            mail_user_context.clone(),
        )))
        .expect(ERR_MSG);
    factory
        .register(Box::new(UnlabelConversationsActionFactory::new(
            mail_user_context.clone(),
        )))
        .expect(ERR_MSG);

    factory
        .register(Box::new(EventLoopActionFactory::new(mail_user_context)))
        .expect(ERR_MSG);

    factory
}
