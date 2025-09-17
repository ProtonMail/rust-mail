use super::CoreContextError;
use super::registry::ServiceRegistry;
use super::services::Service;
use crate::datatypes::ApiConfig;
use crate::os::KeyChain;
use crate::{Context, Origin, UserDatabaseInitializer};
use indexmap::IndexMap;
use proton_task_service::BackgroundAwareTaskService;
use stash::stash::Stash;
use std::any::TypeId;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

enum ServiceUnderConstruction<Err> {
    Simple(Box<dyn Service<Error = Err>>),
    Cyclic(Box<dyn FnOnce(Weak<Context>) -> Box<dyn Service<Error = Err>> + Send + Sync>),
}

#[derive(Default)]
pub struct ContextBuilder {
    services: IndexMap<TypeId, ServiceUnderConstruction<CoreContextError>>,
}

impl ContextBuilder {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_service<T: Service<Error = CoreContextError>>(mut self, service: T) -> Self {
        tracing::info!("Adding service {}", std::any::type_name::<T>());
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
        T: Service<Error = CoreContextError>,
        F: FnOnce(Weak<Context>) -> T + 'static + Send + Sync,
    {
        tracing::info!("Adding service constructor {}", std::any::type_name::<T>());
        self.services.insert(
            TypeId::of::<T>(),
            ServiceUnderConstruction::Cyclic(Box::new(move |this| Box::new(service(this)))),
        );
        self
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn build(
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
    ) -> Result<Arc<Context>, CoreContextError> {
        let this = Arc::new_cyclic(|this| {
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
                service_registry: ServiceRegistry::new(services),
            }
        });

        for service in this.services() {
            service.init().await?;
        }

        Ok(this)
    }
}
