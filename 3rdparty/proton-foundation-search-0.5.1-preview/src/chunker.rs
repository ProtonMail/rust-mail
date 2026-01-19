use std::fmt::Debug;
use std::sync::mpsc::{Receiver, Sender, channel};

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
#[derive(Debug)]
pub struct Chunker<I: Iterator, K, V> {
    receiver: Receiver<(I, Option<I::Item>)>,
    sender: Sender<(I, Option<I::Item>)>,
    key: K,
    value: V,
}

impl<I: Iterator, K, V> Chunker<I, K, V> {
    pub fn new<TK, TV>(iter: I, key: K, value: V) -> Self
    where
        I: Iterator,
        K: Clone + Fn(&I::Item) -> TK,
        V: Clone + Fn(I::Item) -> TV,
    {
        let (sender, receiver) = channel();
        assert!(sender.send((iter, None)).is_ok());
        Self {
            receiver,
            sender,
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

/// A Chunk of the original iterator
///
/// It needs to be exhausted or dropped before the next chunk can be taken
pub struct Chunk<I: Iterator, K, V, TK> {
    current: TK,
    first: Option<I::Item>,
    inner: Option<I>,
    sender: Sender<(I, Option<I::Item>)>,
    key: K,
    value: V,
}
impl<I: Iterator + Debug, K, V, TK: Debug> Debug for Chunk<I, K, V, TK>
where
    I::Item: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Chunk")
            .field("current", &self.current)
            .field("first", &self.first)
            .field("inner", &self.inner)
            .field("sender", &self.sender)
            .finish_non_exhaustive()
    }
}

impl<I: Iterator, K, V, TK> Chunk<I, K, V, TK> {
    fn start<TV>(parent: &mut Chunker<I, K, V>) -> Option<(TK, Self)>
    where
        K: Clone + Fn(&I::Item) -> TK,
        V: Clone + Fn(I::Item) -> TV,
    {
        let (mut occurrences, first) = parent
            .receiver
            .try_recv()
            .inspect_err(|e| {
                tracing::error!(
                    "It is not possible to iterate multiple chunks at the same time - {e}"
                )
            })
            .ok()?;
        let first = first.or_else(|| occurrences.next())?;
        let current = (parent.key)(&first);
        let key = (parent.key)(&first);
        Some((
            key,
            Chunk {
                current,
                first: Some(first),
                inner: Some(occurrences),
                sender: parent.sender.clone(),
                key: parent.key.clone(),
                value: parent.value.clone(),
            },
        ))
    }
}
impl<I: Iterator, K, V, TK> Drop for Chunk<I, K, V, TK> {
    fn drop(&mut self) {
        if let Some(o) = self.inner.take() {
            let _ = self.sender.send((o, self.first.take()));
        }
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
        if let Some(first) = self.first.take() {
            return Some((self.value)(first));
        }
        let mut occurrences = self.inner.take()?;
        let Some(next) = occurrences.next() else {
            let _ = self.sender.send((occurrences, None));
            return None;
        };
        if (self.key)(&next) == self.current {
            self.inner = Some(occurrences);
            Some((self.value)(next))
        } else {
            let _ = self.sender.send((occurrences, Some(next)));
            None
        }
    }
}

#[test]
fn chunker_chunks() {
    let iter = [0, 1, 1, 1, 2, 2, 1].into_iter();
    let sut = Chunker::new(iter, |x| *x, |x| x);
    let res = sut
        .map(|(k, v)| {
            println!("{k} - {v:?}");
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
            println!("{k} - {v:?}");
            (k, v.take(2).collect::<Vec<_>>())
        })
        .collect::<Vec<_>>();
    assert_eq!(
        res,
        vec![(0, vec![0, 0]), (0, vec![0]), (1, vec![1, 1]), (1, vec![1])],
        "in each step we take 2 at most, remaining go into the nexh chunk"
    )
}
