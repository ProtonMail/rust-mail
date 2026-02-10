mod common;
mod payments_impl;
mod request_data;
mod requests;
mod responses;

pub use self::common::*;
pub use self::request_data::*;
pub use self::requests::*;
pub use self::responses::*;
use crate::service::ApiServiceResult;
use bytes::Bytes;

/// The Proton Payments API base path (v5).
pub const PAYMENTS_V5: &str = "/payments/v5";

/// The Proton Payments API base path (v6).
pub const PAYMENTS_V6: &str = "/payments/v6";

#[allow(async_fn_in_trait)]
pub trait ProtonPayments {
    /// Get the payment status. Checks what payment methods are enabled.
    async fn get_payments_status(
        &self,
        vendor: String,
    ) -> ApiServiceResult<GetPaymentsStatusResponse>;

    /// Get the payment plans available to the user.
    async fn get_payments_plans(
        &self,
        options: GetPaymentsPlansOptions,
    ) -> ApiServiceResult<GetPaymentsPlansResponse>;

    /// Get the icon resource with the given name.
    async fn get_payments_resources_icons(&self, name: String) -> ApiServiceResult<Bytes>;

    /// Create a payment token.
    async fn post_payments_tokens(
        &self,
        amount: u64,
        currency: String,
        payment: PaymentReceipt,
    ) -> ApiServiceResult<PostPaymentsTokensResponse>;

    /// Get the current active subscription of the user.
    async fn get_payments_subscription(&self) -> ApiServiceResult<GetPaymentsSubscriptionResponse>;

    /// Create a payment subscription.
    async fn post_payments_subscription(
        &self,
        subscription: NewSubscription,
        new_values: NewSubscriptionValues,
    ) -> ApiServiceResult<()>;

    async fn get_payment_method(
        &self,
        payment_method_id: String,
    ) -> ApiServiceResult<GetPaymentMethodResponse>;
}
