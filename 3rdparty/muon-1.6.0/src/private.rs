pub trait Sealed {}

crate::if_unsealed! {
    impl<T> Sealed for T {}
}
