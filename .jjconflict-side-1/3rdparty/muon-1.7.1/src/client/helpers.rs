use async_trait::async_trait;

/// An async version of `From<T>`.
#[async_trait]
pub trait AsyncFrom<T> {
    /// Constructs `Self` from `value`.
    async fn from(value: T) -> Self;
}

/// An async version of `Into<T>`.
#[async_trait]
pub trait AsyncInto<T> {
    /// Converts `self` into `T`.
    async fn into(self) -> T;
}

/// Blanket implementation of `AsyncInto`.
#[async_trait]
impl<T: Send, U> AsyncInto<U> for T
where
    U: AsyncFrom<T>,
{
    async fn into(self) -> U {
        U::from(self).await
    }
}
