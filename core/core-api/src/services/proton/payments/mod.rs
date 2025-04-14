use bytes::Bytes;

use crate::service::ApiServiceResult;

export! {
    mod common (as pub);
    mod request_data (as pub);
    mod requests (as pub);
    mod response_data (as pub);
    mod responses (as pub);
}

mod payments_impl;

/// The Proton Payments API base path (v5).
pub const PAYMENTS_V5: &str = "/payments/v5";

#[allow(async_fn_in_trait)]
pub trait ProtonPayments {
    /// Get the payment plans available to the user.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    async fn get_payments_plans(
        &self,
        options: GetPaymentsPlansOptions,
    ) -> ApiServiceResult<GetPaymentsPlansResponse>;

    /// Get the icon resource with the given name.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    async fn get_payments_resources_icons(&self, name: String) -> ApiServiceResult<Bytes>;

    /// Create a payment token.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    async fn post_payments_tokens(
        &self,
        amount: u64,
        currency: String,
        payment: PaymentReceipt,
    ) -> ApiServiceResult<PostPaymentsTokensResponse>;

    /// Get the current active subscription of the user.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    async fn get_payments_subscription(&self) -> ApiServiceResult<GetPaymentsSubscriptionResponse>;

    /// Create a payment subscription.
    ///
    /// # Errors
    ///
    /// Returns an error if the request fails.
    async fn post_payments_subscription(
        &self,
        subscription: NewSubscription,
        new_values: NewSubscriptionValues,
    ) -> ApiServiceResult<()>;
}
