use proton_core_api::verification::DynChallengeNotifier;
use std::sync::Arc;

pub struct HvNotifierService {
    notifier: Option<DynChallengeNotifier>,
}

impl HvNotifierService {
    #[must_use]
    pub fn new(notifier: Option<DynChallengeNotifier>) -> Self {
        Self { notifier }
    }

    pub fn notifier_arc(
        &self,
    ) -> Option<Arc<dyn proton_core_api::verification::ChallengeNotifier>> {
        self.notifier.as_ref().map(Arc::clone)
    }
}
