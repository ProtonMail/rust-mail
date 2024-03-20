mod delete_conversations;
mod event_loop;

use crate::WeakMailUserContext;
pub use delete_conversations::*;
pub use event_loop::*;
use proton_action_queue::ActionFactory;

pub(crate) fn new_action_factory(mail_user_context: WeakMailUserContext) -> ActionFactory {
    let mut factory = ActionFactory::new();
    const ERR_MSG: &str = "Double Factory registration";
    factory
        .register(Box::new(DeleteConversationsActionFactory::new(
            mail_user_context.clone(),
        )))
        .expect(ERR_MSG);

    factory
        .register(Box::new(EventLoopActionFactory::new(mail_user_context)))
        .expect(ERR_MSG);

    factory
}
