use crate::UserContext;
use crate::actions::register_actions;

use proton_action_queue::queue::Queue;
use proton_core_api::services::proton::{SessionId, UserId};
use proton_core_api::session::Session;
use stash::UserDb;
use stash::stash::Stash;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use tokio_util::sync::CancellationToken;

use super::Context;
use super::keys::CryptoKeyManager;

enum ServiceUnderConstruction {
    Simple(Box<dyn Any + Send + Sync>),
    Cyclic(Box<dyn FnOnce(Weak<UserContext>) -> Box<dyn Any + Send + Sync> + Send + Sync>),
}

#[derive(Default)]
pub struct UserContextBuilder {
    services: HashMap<TypeId, ServiceUnderConstruction>,
}

impl UserContextBuilder {
    #[must_use]
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
        F: FnOnce(Weak<UserContext>) -> T + Send + Sync + 'static,
    {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceUnderConstruction::Cyclic(Box::new(move |this| Box::new(service(this)))),
        );
        self
    }

    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        self,
        session: Session,
        context: Arc<Context>,
        user_stash: Stash<UserDb>,
        queue: Queue,
        user_id: UserId,
        session_id: SessionId,
        key_manager: Arc<CryptoKeyManager>,
        cancellation_token: CancellationToken,
        cache_path: PathBuf,
    ) -> Arc<UserContext> {
        Arc::new_cyclic(|this| {
            register_actions(context.origin(), &queue, this, &session);

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

            UserContext {
                this: Weak::clone(this),
                session,
                context,
                user_stash,
                queue,
                user_id,
                session_id,
                key_manager,
                cancellation_token,
                cache_path,
                services,
            }
        })
    }
}
