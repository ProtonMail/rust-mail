use core::fmt::Debug;
use std::iter::Peekable;
use std::sync::mpsc::{Receiver, SyncSender, TrySendError, sync_channel};

/// Convenience to create the iterator chunker
pub trait ChunkIter: Iterator + Sized {
    fn chunk<K, V, TK, TV>(self, key: K, value: V) -> Chunker<Self, K, V>
    where
        K: Clone + Fn(&Self::Item) -> TK,
        V: Clone + Fn(Self::Item) -> TV,
    {
        Chunker::new(self, key, value)
    }
}
impl<T: Iterator> ChunkIter for T {}

/// An alternative to itertools ChunkBy where we do not need to collect/allocate
/// but only one chunk can be iterated at a time
///
/// It shares the original iterator between [`Chunk`] and [`Chunker`] using a channel
/// So only one or the other can progress at a time.
pub struct Chunker<I, K, V>
where
    I: Iterator,
{
    starter: Receiver<Peekable<I>>,
    finisher: SyncSender<Peekable<I>>,
    key: K,
    value: V,
}

impl<I, K, V> Chunker<I, K, V>
where
    I: Iterator,
{
    pub fn new<TK, TV>(iter: I, key: K, value: V) -> Self
    where
        I: Iterator,
        K: Clone + Fn(&I::Item) -> TK,
        V: Clone + Fn(I::Item) -> TV,
    {
        let (finisher, starter) = sync_channel(1);
        #[allow(clippy::expect_used)]
        finisher
            .try_send(iter.peekable())
            .expect("must be able to send one item");

        Self {
            starter,
            finisher,
            key,
            value,
        }
    }
}

impl<I, K, V, TK, TV> Iterator for Chunker<I, K, V>
where
    I: Iterator,
    K: Clone + Fn(&I::Item) -> TK,
    V: Clone + Fn(I::Item) -> TV,
{
    type Item = (TK, Chunk<I, K, V, TK>);
    fn next(&mut self) -> Option<Self::Item> {
        Chunk::start(self)
    }
}

impl<I, K, V> Debug for Chunker<I, K, V>
where
    I: Iterator + Debug,
    I::Item: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Self {
            starter,
            finisher: _,
            key: _,
            value: _,
        } = self;
        f.debug_struct("Chunker")
            .field("starter", &starter)
            .finish_non_exhaustive()
    }
}

/// A Chunk of the original iterator
///
/// It needs to be exhausted or dropped before the next chunk can be taken
pub struct Chunk<I, K, V, TK>
where
    I: Iterator,
{
    current: TK,
    inner: Option<Peekable<I>>,
    finisher: SyncSender<Peekable<I>>,
    key: K,
    value: V,
}

impl<I, K, V, TK> Debug for Chunk<I, K, V, TK>
where
    I: Iterator + Debug,
    I::Item: Debug,
    TK: Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let Self {
            current,
            inner,
            finisher,
            key: _,
            value: _,
        } = self;
        f.debug_struct("Chunk")
            .field("current", &current)
            .field("inner", &inner)
            .field("finisher", &finisher)
            .finish_non_exhaustive()
    }
}

impl<I, K, V, TK> Chunk<I, K, V, TK>
where
    I: Iterator,
{
    fn start<TV>(parent: &mut Chunker<I, K, V>) -> Option<(TK, Self)>
    where
        K: Clone + Fn(&I::Item) -> TK,
        V: Clone + Fn(I::Item) -> TV,
    {
        let Some(mut iter) = parent.starter.try_recv().ok() else {
            tracing::error!("It is not possible to iterate multiple chunks at the same time.");
            return None;
        };

        let first = iter.peek()?;
        let current = (parent.key)(first);
        let key = (parent.key)(first);
        Some((
            key,
            Chunk {
                current,
                inner: Some(iter),
                finisher: parent.finisher.clone(),
                key: parent.key.clone(),
                value: parent.value.clone(),
            },
        ))
    }
    fn finish(&mut self) {
        if let Some(o) = self.inner.take()
            && let Err(TrySendError::Full(_)) = self.finisher.try_send(o)
        {
            unreachable!("parent state must be empty when we finish")
        }
    }
}

impl<I, K, V, TK> Drop for Chunk<I, K, V, TK>
where
    I: Iterator,
{
    fn drop(&mut self) {
        self.finish()
    }
}

impl<I, K, V, TK, TV> Iterator for Chunk<I, K, V, TK>
where
    I: Iterator,
    K: Clone + Fn(&I::Item) -> TK,
    V: Clone + Fn(I::Item) -> TV,
    TK: PartialEq,
{
    type Item = TV;
    fn next(&mut self) -> Option<Self::Item> {
        let mut next = || {
            let next = self.inner.as_mut()?.peek()?;
            if (self.key)(next) == self.current {
                let next = self.inner.as_mut()?.next()?;
                Some((self.value)(next))
            } else {
                None
            }
        };

        next().or_else(|| {
            self.finish();
            None
        })
    }
}

#[cfg(test)]
mod tests {
    use tracing::info;

    use super::*;
    use crate::prelude::*;

    #[test]
    fn chunker_chunks() {
        let iter = [0, 1, 1, 1, 2, 2, 1].into_iter();
        let sut = Chunker::new(iter, |x| *x, |x| x);
        let res = sut
            .map(|(k, v)| {
                info!("{k} - {v:?}");
                (k, v.collect::<Vec<_>>())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            res,
            vec![
                (0, vec![0]),
                (1, vec![1, 1, 1]),
                (2, vec![2, 2]),
                (1, vec![1])
            ]
        )
    }

    #[test]
    fn no_multi_chunks() {
        let iter = [0, 1, 1, 1, 2, 2, 1].into_iter();
        let sut = Chunker::new(iter, |x| *x, |x| x);
        let chunks = sut.collect::<Vec<_>>();

        assert_eq!(chunks.len(), 1)
    }

    #[test]
    fn incomplete_chunk_iteration() {
        let iter = [0, 0, 0, 1, 1, 1].into_iter();
        let sut = Chunker::new(iter, |x| *x, |x| x);
        let res = sut
            .map(|(k, v)| {
                info!("{k} - {v:?}");
                (k, v.take(2).collect::<Vec<_>>())
            })
            .collect::<Vec<_>>();
        assert_eq!(
            res,
            vec![(0, vec![0, 0]), (0, vec![0]), (1, vec![1, 1]), (1, vec![1])],
            "in each step we take 2 at most, remaining go into the nexh chunk"
        )
    }

    #[test]
    fn chunker_is_send() {
        fn sendy(_: impl Send) {}

        fn chunky(iter: impl Send + Iterator<Item = impl Send>) {
            let chunker = iter.chunk(|_c| &(), |c| c);
            sendy(chunker);
        }

        chunky(0..10)
    }
}
