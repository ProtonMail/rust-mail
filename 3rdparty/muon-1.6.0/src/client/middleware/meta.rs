use crate::common::{BoxFut, Sender, SenderLayer};
use crate::http::{HttpReq, HttpRes};
use crate::Result;
use derive_more::{AsRef, Deref, Display};
use muon_proc::autoimpl;
use std::borrow::Borrow;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::Relaxed;

/// A tag applied to every request by the `Tagger` layer.
#[derive(Debug, Display, AsRef, Deref, Clone, Copy)]
#[derive(PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Tag(usize);

impl Tag {
    /// Get the tag of a request, if one is present.
    #[must_use]
    pub fn get(req: &HttpReq) -> Option<&Self> {
        req.get_extension()
    }
}

/// A trait for getting the tag of a request.
#[autoimpl]
pub trait GetTag: Borrow<HttpReq> {
    /// Get the tag of the request, if any.
    ///
    /// The tag is assigned by the [`Tagger`] layer.
    fn get_tag(&self) -> Option<&Tag> {
        Tag::get(self.borrow())
    }
}

/// A layer that tags every request with a unique identifier.
#[must_use]
#[derive(Debug, Default)]
pub struct Tagger {
    next: AtomicUsize,
}

impl SenderLayer<HttpReq, HttpRes> for Tagger {
    fn on_send<'a>(
        &'a self,
        inner: &'a dyn Sender<HttpReq, HttpRes>,
        req: HttpReq,
    ) -> BoxFut<'a, Result<HttpRes>> {
        let tag = Tag(self.next.fetch_add(1, Relaxed));
        let req = req.extension(tag);

        inner.send(req)
    }
}
