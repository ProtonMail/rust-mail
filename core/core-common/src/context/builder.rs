use crate::core_clock::CoreClock;
use crate::datatypes::ApiConfig;
use crate::os::KeyChain;
use crate::{Context, Origin, UserDatabaseInitializer};
use proton_log_service::LogService;
use proton_task_service::BackgroundAwareTaskService;
use stash::stash::Stash;
use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

enum ServiceUnderConstruction {
    Simple(Box<dyn Any + Send + Sync>),
    Cyclic(Box<dyn FnOnce(Weak<Context>) -> Box<dyn Any + Send + Sync> + Send + Sync>),
}

#[derive(Default)]
pub struct ContextBuilder {
    services: HashMap<TypeId, ServiceUnderConstruction>,
}

impl ContextBuilder {
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

    /// Whenever a service must be constructed with `Weak<Context>` as an argument.
    /// It's initialization is delayed until `Self::build` is called.
    #[must_use]
    pub fn with_cyclic_service<T, F>(mut self, service: F) -> Self
    where
        T: Any + Send + Sync + 'static,
        F: FnOnce(Weak<Context>) -> T + 'static + Send + Sync,
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
        origin: Origin,
        user_db_path: PathBuf,
        account_db_path: PathBuf,
        cache_path: PathBuf,
        api_config: ApiConfig,
        account_stash: Stash,
        key_chain: Arc<dyn KeyChain>,
        user_db_initializers: Vec<Box<dyn UserDatabaseInitializer>>,
        task_service: BackgroundAwareTaskService,
        clock: CoreClock,
        log_service: LogService,
    ) -> Arc<Context> {
        Arc::new_cyclic(|this| {
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

            Context {
                this: Weak::clone(this),
                active_user_contexts: Mutex::new(HashMap::new()),
                origin,
                user_db_path,
                account_db_path,
                cache_path,
                api_config,
                account_stash,
                key_chain,
                cancellation_token: CancellationToken::new(),
                user_db_initializers,
                task_service,
                clock,
                log_service,
                services,
            }
        })
    }
}
