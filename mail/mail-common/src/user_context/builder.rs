use crate::actions::register_actions;

use crate::{MailContext, MailContextResult};
use proton_core_api::session::CoreSession;
use proton_core_common::UserContext;

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::Arc;

use super::MailUserContext;

pub struct MailUserContextBuilder {
    services: HashMap<TypeId, Box<dyn Any + Send + Sync>>,
}

impl MailUserContextBuilder {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    pub fn with_service<T: Any + Send + Sync + 'static>(mut self, service: T) -> Self {
        self.services.insert(TypeId::of::<T>(), Box::new(service));
        self
    }

    pub async fn build(
        self,
        mail_context: Arc<MailContext>,
        user_context: Arc<UserContext>,
    ) -> MailContextResult<Arc<MailUserContext>> {
        let origin = mail_context.core_context().origin();

        let this = Arc::new_cyclic(|weak_self| {
            register_actions(
                user_context.queue(),
                origin,
                weak_self,
                user_context.session().api(),
            );

            MailUserContext {
                this: weak_self.clone(),
                mail_context,
                user_context,
                services: self.services,
            }
        });

        Ok(this)
    }
}
