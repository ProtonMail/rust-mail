use crate::Result;
use async_trait::async_trait;
use bytes::Bytes;
use derive_more::Debug;
use futures::future;
use http_body::{Body as HttpBody, Frame, SizeHint};
use muon_proc::autoimpl;
use pin_project::pin_project;
use std::convert::Infallible;
use std::pin::{pin, Pin};
use std::task::{Context, Poll};

/// The poll result of an HTTP body.
type BodyPoll<T, E> = Poll<Option<Result<Frame<T>, E>>>;

/// A request or response body.
#[pin_project]
#[derive(Debug, Default)]
pub struct Body {
    data: Option<Bytes>,
}

impl Body {
    /// Create a new body from the given bytes.
    pub fn new(data: impl Into<Bytes>) -> Self {
        Self {
            data: Some(data.into()),
        }
    }
}

impl From<Body> for Bytes {
    fn from(body: Body) -> Self {
        body.data.unwrap_or_default()
    }
}

impl HttpBody for Body {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(mut self: Pin<&mut Self>, _: &mut Context) -> BodyPoll<Self::Data, Self::Error> {
        if let Some(data) = self.data.take() {
            Poll::Ready(Some(Ok(Frame::data(data))))
        } else {
            Poll::Ready(None)
        }
    }

    fn is_end_stream(&self) -> bool {
        self.data.is_none()
    }

    fn size_hint(&self) -> SizeHint {
        if let Some(data) = &self.data {
            SizeHint::with_exact(data.len() as u64)
        } else {
            SizeHint::with_exact(0)
        }
    }
}

/// Collects the body of a streaming response into a single buffer.
#[autoimpl]
#[async_trait]
pub trait Collect<B>: Into<http::Response<B>> + Sized
where
    B: HttpBody + Send + Unpin,
    B::Data: Into<Vec<u8>>,
{
    /// Collect the body of the response into a single buffer.
    async fn collect(self) -> Result<http::Response<Vec<u8>>, B::Error> {
        let mut this = self.into();

        let body = collect(this.body_mut()).await?;

        Ok(this.map(|_| body))
    }
}

async fn collect<B>(body: B) -> Result<Vec<u8>, B::Error>
where
    B: HttpBody,
    B::Data: Into<Vec<u8>>,
{
    let mut body = pin!(body);
    let mut bufs = Vec::new();

    while let Some(data) = future::poll_fn(|cx| body.as_mut().poll_frame(cx)).await {
        if let Ok(data) = data?.into_data() {
            bufs.extend(data.into());
        }
    }

    Ok(bufs)
}
