This crate provides a [`ShadowCountedIter`] which counts every iteration to a hidden
counter. It is possible to create nested iterators which can commit their counter to their parent
iterator.

Unlike the std [`std::iter::Enumerate`] iterator, the [`ShadowCountedIter`] does not return the counter
to the user, instead it has to be queried with the [`ShadowCountedIter::counter()`] method.

We also provide a [`IntoShadowCounted`] extension trait which converts any iterator into a
[`ShadowCountedIter`].


## Examples

### Basic Counting

```rust
use shadow_counted::{ShadowCountedIter, IntoShadowCounted};

let vec = vec![1, 2, 3];
let mut iter = vec.into_iter().shadow_counted();
while let Some(_) = iter.next() {}
assert_eq!(iter.counter(), 3);
```

### Nested Counting

```rust
use shadow_counted::{ShadowCountedIter, IntoShadowCounted};

// Make a datastructure that may hold nested elements.
#[derive(Debug, PartialEq)]
enum Nodes<'a, T> {
    Leaf(T),
    Nested(&'a [Nodes<'a, T>]),
}

let items = &[
    Nodes::Leaf(1),
    Nodes::Nested(&[Nodes::Leaf(2), Nodes::Leaf(3)]),
    Nodes::Leaf(4),
];

// iterate over the outer
let mut sc_iter = items.into_iter().shadow_counted();
assert_eq!(sc_iter.next(), Some(&Nodes::Leaf(1)));

// the 2nd element is `Node::Nested(..)'
let element = sc_iter.next().unwrap();
# assert_eq!(element, &Nodes::Nested(&[Nodes::Leaf(2), Nodes::Leaf(3)]));

// since we dont want to count `Nested` we substract one from the counter
sc_iter.add(-1);
let Nodes::Nested(nested) = element else {unreachable!()};
let mut nested_iter = nested.into_iter().nested_shadow_counted(&mut sc_iter);

# assert_eq!(nested_iter.counter(), 1);
assert_eq!(nested_iter.next(), Some(&Nodes::Leaf(2)));
# assert_eq!(nested_iter.counter(), 2);
assert_eq!(nested_iter.next(), Some(&Nodes::Leaf(3)));
# assert_eq!(nested_iter.counter(), 3);
// reaching the end, commit to the parent iter
assert_eq!(nested_iter.next(), None);

// eventually a nested iter must be committed when its progress should be counted
nested_iter.commit();

// back to the outer
assert_eq!(sc_iter.counter(), 3);
assert_eq!(sc_iter.next(), Some(&Nodes::Leaf(4)));
# assert_eq!(sc_iter.counter(), 4);
# assert_eq!(sc_iter.next(), None);
assert_eq!(sc_iter.counter(), 4);
```
