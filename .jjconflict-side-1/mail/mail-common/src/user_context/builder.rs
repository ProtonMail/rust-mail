use crate::actions::register_actions;

use crate::{MailContext, MailContextResult};
use proton_core_common::UserContext;

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::sync::{Arc, Weak};

use super::MailUserContext;

enum ServiceUnderConstruction {
    Simple(Box<dyn Any + Send + Sync>),
    Cyclic(Box<dyn FnOnce(Weak<MailUserContext>) -> Box<dyn Any + Send + Sync> + Send + Sync>),
}

pub struct MailUserContextBuilder {
    services: HashMap<TypeId, ServiceUnderConstruction>,
}

impl MailUserContextBuilder {
    pub fn new() -> Self {
        Self {
            services: HashMap::new(),
        }
    }

    #[must_use]
    pub fn with_service<T: Any + Send + Sync + 'static>(mut self, service: T) -> Self {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceUnderConstruction::Simple(Box::new(service)),
        );
        self
    }

    /// Whenever a service must be constructed with `Weak<UserContext>` as an argument.
    /// It's initialization is delayed until `Self::build` is called.
    #[must_use]
    pub fn with_cyclic_service<T, F>(mut self, service: F) -> Self
    where
        T: Any + Send + Sync + 'static,
        F: FnOnce(Weak<MailUserContext>) -> T + Send + Sync + 'static,
    {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceUnderConstruction::Cyclic(Box::new(move |this| Box::new(service(this)))),
        );
        self
    }

    pub async fn build(
        self,
        mail_context: Arc<MailContext>,
        user_context: Arc<UserContext>,
    ) -> MailContextResult<Arc<MailUserContext>> {
        let origin = mail_context.core_context().origin();

        let this = Arc::new_cyclic(|this| {
            register_actions(
                user_context.queue(),
                origin,
                this,
                user_context.session(),
                mail_context.http_client(),
            );

            let services = self
                .services
                .into_iter()
                .map(|(type_id, service)| match service {
                    ServiceUnderConstruction::Cyclic(f) => {
                        let service = f(Weak::clone(this));
                        (type_id, service)
                    }
                    ServiceUnderConstruction::Simple(service) => (type_id, service),
                })
                .collect();

            MailUserContext {
                this: Weak::clone(this),
                mail_context,
                user_context,
                services,
            }
        });

        Ok(this)
    }
}
