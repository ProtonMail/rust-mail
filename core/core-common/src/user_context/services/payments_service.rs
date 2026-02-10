use std::sync::{Arc, Weak};

use anyhow::anyhow;
use bytes::Bytes;
use proton_core_api::services::proton::{
    GetPaymentMethodResponse, GetPaymentsPlansOptions, GetPaymentsPlansResponse,
    GetPaymentsStatusResponse, GetPaymentsSubscriptionResponse, NewSubscription,
    NewSubscriptionValues, PaymentReceipt, PostPaymentsTokensResponse, ProtonPayments,
};

use crate::{CoreContextError, CoreContextResult, UserContext};

pub struct PaymentsService {
    ctx: Weak<UserContext>,
}

impl PaymentsService {
    #[must_use]
    pub fn new(ctx: Weak<UserContext>) -> Self {
        Self { ctx }
    }

    #[allow(clippy::result_large_err)]
    fn ctx(&self) -> CoreContextResult<Arc<UserContext>> {
        self.ctx
            .upgrade()
            .ok_or_else(|| CoreContextError::Other(anyhow!("Missing context")))
    }

    pub async fn get_payments_status(
        &self,
        vendor: String,
    ) -> CoreContextResult<GetPaymentsStatusResponse> {
        let res = self.ctx()?.session().get_payments_status(vendor).await?;
        Ok(res)
    }

    pub async fn get_payments_plans(
        &self,
        options: GetPaymentsPlansOptions,
    ) -> CoreContextResult<GetPaymentsPlansResponse> {
        let res = self.ctx()?.session().get_payments_plans(options).await?;
        Ok(res)
    }

    pub async fn get_payments_resources_icons(&self, name: String) -> CoreContextResult<Bytes> {
        let res = self
            .ctx()?
            .session()
            .get_payments_resources_icons(name)
            .await?;
        Ok(res)
    }

    pub async fn post_payments_tokens(
        &self,
        amount: u64,
        currency: String,
        payment: PaymentReceipt,
    ) -> CoreContextResult<PostPaymentsTokensResponse> {
        let res = self
            .ctx()?
            .session()
            .post_payments_tokens(amount, currency, payment)
            .await?;
        Ok(res)
    }

    pub async fn get_payments_subscription(
        &self,
    ) -> CoreContextResult<GetPaymentsSubscriptionResponse> {
        let res = self.ctx()?.session().get_payments_subscription().await?;
        Ok(res)
    }

    pub async fn post_payments_subscription(
        &self,
        subscribtion: NewSubscription,
        new_values: NewSubscriptionValues,
    ) -> CoreContextResult<()> {
        self.ctx()?
            .session()
            .post_payments_subscription(subscribtion, new_values)
            .await?;
        Ok(())
    }

    pub async fn get_payment_method(
        &self,
        id: String,
    ) -> CoreContextResult<GetPaymentMethodResponse> {
        let res = self.ctx()?.session().get_payment_method(id).await?;
        Ok(res)
    }
}
