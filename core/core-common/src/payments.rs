use proton_core_api::{
    service::ApiServiceResult,
    services::proton::{GetPaymentsStatusResponse, ProtonPayments},
};

use crate::UserContext;

impl UserContext {
    pub async fn get_payments_status(
        &self,
        vendor: String,
    ) -> ApiServiceResult<GetPaymentsStatusResponse> {
        self.session().get_payments_status(vendor).await
    }
}
