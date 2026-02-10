use std::any::{TypeId, type_name};

use indexmap::IndexMap;

use super::services::Service;

pub struct ServiceRegistry<Error> {
    services: IndexMap<TypeId, Box<dyn Service<Error = Error>>>,
}

impl<Err: Send + Sync + 'static> ServiceRegistry<Err> {
    pub fn new(services: IndexMap<TypeId, Box<dyn Service<Error = Err>>>) -> Self {
        Self { services }
    }

    pub fn services(&self) -> impl Iterator<Item = &Box<dyn Service<Error = Err> + 'static>> {
        self.services.values()
    }

    #[allow(clippy::result_large_err)]
    /// # Panics
    /// This function panics if the service is not found.
    /// If there is a need for a service that may not exist, use `get_service_opt`.
    pub fn get_service<T: 'static>(&self) -> &T {
        self.get_service_opt::<T>()
            .unwrap_or_else(|| panic!("Service {} not found", type_name::<T>()))
    }

    #[allow(clippy::result_large_err)]
    pub fn get_service_opt<T: 'static>(&self) -> Option<&T> {
        let type_id = TypeId::of::<T>();
        tracing::trace!("Retrieving {}. type ID: {:?}", type_name::<T>(), type_id);
        self.services.get(&type_id).map(|service| {
            <dyn std::any::Any>::downcast_ref(&**service).unwrap_or_else(|| {
                panic!(
                    "Could not downcast_ref. Service {} - type ID {:?}",
                    type_name::<T>(),
                    type_id
                )
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestService;
    #[async_trait::async_trait]
    impl Service for TestService {
        type Error = ();
    }

    #[test]
    fn downcasting_works() {
        let mut services = IndexMap::new();
        let service: Box<dyn Service<Error = ()>> = Box::new(TestService);
        services.insert(TypeId::of::<TestService>(), service);
        let registry = ServiceRegistry::new(services);

        registry.get_service::<TestService>();
    }
}
