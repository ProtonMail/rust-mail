/// Specifies the way to output type parameters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeParamsFmtOpts {
    /// Output all type parameters, with the `m`/`n` number of segments.
    All,
    /// Only output type parameters if the type is from the standard library.
    ///
    /// # Examples
    ///
    /// * `MyStruct<SomeType>` returns `MyStruct`.
    /// * `Vec<SomeType>` returns `Vec<SomeType>`.
    /// * `Pin<Box<SomeType>>` returns `Pin<Box<SomeType>>`.
    /// * `Box<dyn MyTrait<SomeType>>` returns `Box<dyn MyTrait>`.
    Std,
}
