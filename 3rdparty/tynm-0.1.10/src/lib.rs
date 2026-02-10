#![no_std]
// #![deny(missing_docs, missing_debug_implementations)]

//! Returns type names with a specifiable number of module segments as a `String`.
//!
//! # Usage
//!
//! Add the following to `Cargo.toml`
//!
//! ```toml
//! tynm = "0.1.10"
//! ```
//!
//! In code:
//!
//! ```rust
//! # fn main() {
//! #[rustfmt::skip]
//! assert_eq!(
//!     core::any::type_name::<Option<String>>(), "core::option::Option<alloc::string::String>"
//! );
//!
//! #[rustfmt::skip]
//! let tuples = vec![
//!     (tynm::type_name::<Option<String>>(),    "Option<String>"),
//!     (tynm::type_namem::<Option<String>>(1),  "core::..::Option<alloc::..::String>"),
//!     (tynm::type_namen::<Option<String>>(1),  "..::option::Option<..::string::String>"),
//!     // 1 segment from most and least significant modules.
//!     (tynm::type_namemn::<rust_out::two::three::Struct>(1, 1), "rust_out::..::three::Struct"),
//!     // traits
//!     (tynm::type_name::<dyn core::fmt::Debug>(), "dyn Debug"),
//! ];
//!
//! tuples
//!     .iter()
//!     .for_each(|(left, right)| assert_eq!(left, right));
//! # }
//!
//! # #[rustfmt::skip]
//! # mod rust_out { pub mod two { pub mod three { pub struct Struct; } } }
//! ```
//!
//! # Motivation
//!
//! The [`core::any::type_name`] function stabilized in Rust 1.38 returns the fully qualified type
//! name with all module segments. This can be difficult to read in error messages, especially for
//! type-parameterized types.
//!
//! Often, the simple type name is more readable, and enough to distinguish the type referenced in
//! an error.
//!
//! [`core::any::type_name`]: https://doc.rust-lang.org/std/any/fn.type_name.html

extern crate alloc;

use alloc::string::String;

pub use crate::{
    type_params_fmt_opts::TypeParamsFmtOpts,
    types::{TypeName, TypeNameDisplay},
};

#[cfg(feature = "info")]
pub use crate::type_name_info::TypeNameInfo;

mod parser;
mod type_params_fmt_opts;
mod types;

#[cfg(feature = "info")]
mod type_name_info;

/// Returns the simple type name.
///
/// # Type Parameters
///
/// * `T`: Type whose simple type name should be returned.
///
/// # Examples
///
/// ```rust
/// assert_eq!(tynm::type_name::<Option<String>>(), "Option<String>",);
/// ```
pub fn type_name<T>() -> String
where
    T: ?Sized,
{
    type_namemn::<T>(0, 0)
}

/// Returns the simple type name.
///
/// # Parameters
///
/// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
///
/// # Type Parameters
///
/// * `T`: Type whose simple type name should be returned.
///
/// # Examples
///
/// ```rust
/// # use tynm::TypeParamsFmtOpts;
/// struct MyStruct<T>(T);
///
/// # fn main() {
/// assert_eq!(
///     tynm::type_name_opts::<MyStruct<String>>(TypeParamsFmtOpts::All),
///     "MyStruct<String>",
/// );
/// assert_eq!(
///     tynm::type_name_opts::<MyStruct<String>>(TypeParamsFmtOpts::Std),
///     "MyStruct",
/// );
/// assert_eq!(
///     tynm::type_name_opts::<Vec<MyStruct<String>>>(TypeParamsFmtOpts::Std),
///     "Vec<MyStruct>",
/// );
/// # }
/// ```
pub fn type_name_opts<T>(type_params_fmt_opts: TypeParamsFmtOpts) -> String
where
    T: ?Sized,
{
    type_namemn_opts::<T>(0, 0, type_params_fmt_opts)
}

/// Returns the type name with at most `m` most significant module path segments.
///
/// # Parameters
///
/// * `m`: Number of most significant module path segments to include.
///
/// # Type Parameters
///
/// * `T`: Type whose simple type name should be returned.
///
/// # Examples
///
/// ```rust
/// assert_eq!(
///     tynm::type_namem::<Option<String>>(1),
///     "core::..::Option<alloc::..::String>",
/// );
/// ```
pub fn type_namem<T>(m: usize) -> String
where
    T: ?Sized,
{
    type_namemn::<T>(m, 0)
}

/// Returns the type name with at most `m` most significant module path segments.
///
/// # Parameters
///
/// * `m`: Number of most significant module path segments to include.
/// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
///
/// # Type Parameters
///
/// * `T`: Type whose simple type name should be returned.
///
/// # Examples
///
/// ```rust
/// # use tynm::TypeParamsFmtOpts;
/// pub mod a {
///     pub mod b {
///         pub mod c {
///             pub struct MyStruct<T>(T);
///         }
///     }
/// }
///
/// # fn main() {
/// # use crate::a::b::c::MyStruct;
/// assert_eq!(
///     tynm::type_namem_opts::<MyStruct<String>>(1, TypeParamsFmtOpts::All),
///     "rust_out::..::MyStruct<alloc::..::String>",
/// );
/// assert_eq!(
///     tynm::type_namem_opts::<MyStruct<String>>(1, TypeParamsFmtOpts::Std),
///     "rust_out::..::MyStruct",
/// );
/// # }
/// ```
pub fn type_namem_opts<T>(m: usize, type_params_fmt_opts: TypeParamsFmtOpts) -> String
where
    T: ?Sized,
{
    type_namemn_opts::<T>(m, 0, type_params_fmt_opts)
}

/// Returns the type name with at most `n` least significant module path segments.
///
/// # Parameters
///
/// * `n`: Number of least significant module path segments to include.
///
/// # Type Parameters
///
/// * `T`: Type whose simple type name should be returned.
///
/// # Examples
///
/// ```rust
/// assert_eq!(
///     tynm::type_namen::<Option<String>>(1),
///     "..::option::Option<..::string::String>",
/// );
/// ```
pub fn type_namen<T>(n: usize) -> String
where
    T: ?Sized,
{
    type_namemn::<T>(0, n)
}

/// Returns the type name with at most `n` least significant module path segments.
///
/// # Parameters
///
/// * `n`: Number of least significant module path segments to include.
/// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
///
/// # Type Parameters
///
/// * `T`: Type whose simple type name should be returned.
///
/// # Examples
///
/// ```rust
/// # use tynm::TypeParamsFmtOpts;
/// pub mod a {
///     pub mod b {
///         pub mod c {
///             pub struct MyStruct<T>(T);
///         }
///     }
/// }
///
/// # fn main() {
/// # use crate::a::b::c::MyStruct;
/// assert_eq!(
///     tynm::type_namen_opts::<MyStruct<String>>(1, TypeParamsFmtOpts::Std),
///     "..::c::MyStruct",
/// );
/// # }
/// ```
pub fn type_namen_opts<T>(n: usize, type_params_fmt_opts: TypeParamsFmtOpts) -> String
where
    T: ?Sized,
{
    type_namemn_opts::<T>(0, n, type_params_fmt_opts)
}

/// Returns the type name with `m` most significant, and `n` least significant module path segments.
///
/// # Parameters
///
/// * `m`: Number of most significant module path segments to include.
/// * `n`: Number of least significant module path segments to include.
///
/// # Type Parameters
///
/// * `T`: Type whose simple type name should be returned.
///
/// # Examples
///
/// ```rust
/// assert_eq!(
///     tynm::type_namen::<Option<String>>(1),
///     "..::option::Option<..::string::String>",
/// );
/// ```
pub fn type_namemn<T>(m: usize, n: usize) -> String
where
    T: ?Sized,
{
    type_namemn_opts::<T>(m, n, TypeParamsFmtOpts::All)
}

/// Returns the type name with `m` most significant, and `n` least significant module path segments.
///
/// # Parameters
///
/// * `m`: Number of most significant module path segments to include.
/// * `n`: Number of least significant module path segments to include.
/// * `type_params_fmt_opts`: How to format type parameters, see the type documentation for details.
///
/// # Type Parameters
///
/// * `T`: Type whose simple type name should be returned.
///
/// # Examples
///
/// ```rust
/// # use tynm::TypeParamsFmtOpts;
/// pub mod a {
///     pub mod b {
///         pub mod c {
///             pub struct MyStruct<T>(T);
///         }
///     }
/// }
///
/// # fn main() {
/// # use crate::a::b::c::MyStruct;
/// assert_eq!(
///     tynm::type_namemn_opts::<MyStruct<String>>(1, 1, TypeParamsFmtOpts::Std),
///     "rust_out::..::c::MyStruct",
/// );
/// # }
/// ```
pub fn type_namemn_opts<T>(m: usize, n: usize, type_params_fmt_opts: TypeParamsFmtOpts) -> String
where
    T: ?Sized,
{
    let type_name_qualified = core::any::type_name::<T>();

    let type_name = TypeName::from(type_name_qualified);
    type_name.as_str_mn_opts(m, n, type_params_fmt_opts)
}

#[cfg(test)]
mod tests {
    use alloc::{boxed::Box, format, string::String, vec::Vec};

    use super::{TypeName, TypeParamsFmtOpts};
    use crate as tynm;

    #[test]
    fn type_name_primitives() {
        assert_eq!(tynm::type_name::<usize>(), "usize");
        assert_eq!(tynm::type_name::<u8>(), "u8");
        assert_eq!(tynm::type_name::<u16>(), "u16");
        assert_eq!(tynm::type_name::<u32>(), "u32");
        assert_eq!(tynm::type_name::<u64>(), "u64");
        assert_eq!(tynm::type_name::<u128>(), "u128");

        assert_eq!(tynm::type_name::<isize>(), "isize");
        assert_eq!(tynm::type_name::<i8>(), "i8");
        assert_eq!(tynm::type_name::<i16>(), "i16");
        assert_eq!(tynm::type_name::<i32>(), "i32");
        assert_eq!(tynm::type_name::<i64>(), "i64");
        assert_eq!(tynm::type_name::<i128>(), "i128");

        assert_eq!(tynm::type_name::<f32>(), "f32");
        assert_eq!(tynm::type_name::<f64>(), "f64");

        assert_eq!(tynm::type_name::<bool>(), "bool");
        assert_eq!(tynm::type_name::<char>(), "char");

        assert_eq!(
            tynm::type_name::<Vec<(u32, String)>>(),
            "Vec<(u32, String)>"
        );
    }

    #[test]
    fn type_name_array() {
        assert_eq!(tynm::type_name::<[u32; 3]>(), "[u32; 3]");
        assert_eq!(
            tynm::type_name::<[Option<String>; 3]>(),
            "[Option<String>; 3]"
        );
    }

    #[test]
    fn type_name_opts() {
        struct MyStruct<T>(T);

        assert_eq!(
            tynm::type_name_opts::<MyStruct<String>>(TypeParamsFmtOpts::All),
            "MyStruct<String>"
        );
        assert_eq!(
            tynm::type_name_opts::<MyStruct<String>>(TypeParamsFmtOpts::Std),
            "MyStruct",
        );
        assert_eq!(
            tynm::type_name_opts::<Vec<MyStruct<String>>>(TypeParamsFmtOpts::Std),
            "Vec<MyStruct>",
        );
    }

    #[test]
    fn type_namem_opts() {
        struct MyStruct<T>(T);

        assert_eq!(
            tynm::type_namem_opts::<MyStruct<String>>(1, TypeParamsFmtOpts::All),
            "tynm::..::MyStruct<alloc::..::String>",
        );
        assert_eq!(
            tynm::type_namem_opts::<MyStruct<String>>(1, TypeParamsFmtOpts::Std),
            "tynm::..::MyStruct",
        );
    }

    #[test]
    fn type_namemn_opts() {
        struct MyStruct<T>(T);

        assert_eq!(
            tynm::type_namemn_opts::<MyStruct<String>>(1, 1, TypeParamsFmtOpts::Std),
            "tynm::..::type_namemn_opts::MyStruct",
        );
    }

    #[test]
    fn type_name_slice() {
        assert_eq!(tynm::type_name::<&[u32]>(), "&[u32]");
    }

    #[test]
    fn type_name_unit_tuple() {
        assert_eq!(tynm::type_name::<()>(), "()");
        assert_eq!(tynm::type_name::<(Option<String>,)>(), "(Option<String>,)");
        assert_eq!(
            tynm::type_name::<(Option<String>, u32)>(),
            "(Option<String>, u32)"
        );
    }

    #[test]
    fn type_name_reference() {
        assert_eq!(tynm::type_name::<&str>(), "&str");

        assert_eq!(tynm::type_name::<&Option<String>>(), "&Option<String>");
        assert_eq!(tynm::type_name::<Option<&String>>(), "Option<&String>");
    }

    #[test]
    fn type_name_display() {
        use core::sync::atomic::AtomicI8;

        type T<'a> = Box<Vec<(Option<String>, &'a AtomicI8)>>;

        let tn: TypeName = TypeName::new::<T>();

        let display = tn.as_display();
        let string = format!("{display}");
        assert_eq!(tynm::type_name::<T>(), string);
    }

    #[test]
    fn type_name_display_mn() {
        use core::sync::atomic::AtomicI8;

        type T<'a> = Box<Vec<(Option<String>, &'a AtomicI8)>>;

        let tn: TypeName = TypeName::new::<T>();

        let display = tn.as_display_mn(1, 0);
        let string = format!("{display}");
        assert_eq!(tynm::type_namemn::<T>(1, 0), string);
    }

    #[test]
    fn type_name_usize_mn() {
        assert_eq!(tynm::type_namem::<usize>(core::usize::MAX), "::usize");
        assert_eq!(
            tynm::type_namemn::<usize>(core::usize::MAX, core::usize::MAX),
            "::usize"
        );
    }

    #[test]
    fn type_name_unsized() {
        assert_eq!(tynm::type_name::<dyn core::fmt::Debug>(), "dyn Debug");
    }

    #[test]
    fn type_name_unsized_mn() {
        assert_eq!(
            tynm::type_namem::<dyn core::fmt::Debug>(1),
            "dyn core::..::Debug"
        );
        assert_eq!(
            tynm::type_namen::<dyn core::fmt::Debug>(1),
            "dyn ..::fmt::Debug"
        );
        assert_eq!(
            tynm::type_namemn::<dyn core::fmt::Debug>(0, 1),
            "dyn ..::fmt::Debug"
        );
    }
}
