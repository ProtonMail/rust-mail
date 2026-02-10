/// Create a `Url` from arguments that can be formatted with `format!`.
#[macro_export]
#[doc(hidden)]
macro_rules! url {
    ($($tt:tt)*) => {
        $crate::deps::url::Url::parse(&format!($($tt)*))
    };
}

/// Given one or more modules, re-exports all of their public items.
#[macro_export]
#[doc(hidden)]
macro_rules! export {
    () => {};

    ($(#[$meta:meta])* $vis:vis mod $name:ident $((as $out:vis))?; $($rest:tt)*) => {
        $(#[$meta])* $vis mod $name;

        $crate::export!(@ $name, $($out)?, $($rest)*);
    };

    ($(#[$meta:meta])* $vis:vis mod $name:ident $((as $out:vis))? { $($item:item)* } $($rest:tt)*) => {
        $(#[$meta])* $vis mod $name { $($item)* }

        $crate::export!(@ $name, $($out)?, $($rest)*);
    };

    (@ $name:ident, $out:vis, $($rest:tt)*) => {
        #[allow(unused_imports)]
        $out use self::$name::*;

        $crate::export!($($rest)*);
    };
}
