use crate::service::ApiServiceResult;
use crate::services::proton::payments::{ProtonPayments, PAYMENTS_V5};
use crate::services::proton::prelude::*;
use crate::services::proton::Proton;
use muon::{serde_to_query, util::ProtonRequestExt, GET, POST};

impl ProtonPayments for Proton {
    async fn get_payments_plans(
        &self,
        options: GetPaymentsPlansOptions,
    ) -> ApiServiceResult<GetPaymentsPlansResponse> {
        Ok(GET!("{PAYMENTS_V5}/plans")
            .query(serde_to_query(options)?)
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn post_payments_tokens(
        &self,
        amount: u64,
        currency: String,
        payment: PaymentReceipt,
    ) -> ApiServiceResult<PostPaymentsTokensResponse> {
        Ok(POST!("{PAYMENTS_V5}/tokens")
            .body_json(PostPaymentsTokensRequest {
                amount,
                currency,
                payment,
            })?
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn get_payments_subscription(&self) -> ApiServiceResult<GetPaymentsSubscriptionResponse> {
        Ok(GET!("{PAYMENTS_V5}/subscription")
            .send_with(self)
            .await?
            .ok()?
            .into_body_json()?)
    }

    async fn post_payments_subscription(
        &self,
        subscription: NewSubscription,
        new_values: NewSubscriptionValues,
    ) -> ApiServiceResult<()> {
        POST!("{PAYMENTS_V5}/subscription")
            .body_json(PostPaymentsSubscriptionRequest {
                subscription,
                new_values,
            })?
            .send_with(self)
            .await?
            .ok()?;

        Ok(())
    }
}
