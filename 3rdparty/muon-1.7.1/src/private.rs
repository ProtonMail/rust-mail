pub trait Sealed {}

if_unsealed! {
    impl<T> Sealed for T {}
}
