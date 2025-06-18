use super::*;

/// Creates an *.ics-compatible string.
///
/// This macro just replaces newlines (`\n`) with the expected ones (`\r\n`), it
/// doesn't validate given string.
///
/// # Example
///
/// ```
/// # use proton_ical::ics;
/// #
/// ics! {"
///     BEGIN:VEVENT
///     something something
///     END:VEVENT
/// "};
/// ```
#[macro_export]
macro_rules! ics {
    ($str:literal) => {
        indoc::indoc!($str).lines().collect::<Vec<_>>().join("\r\n")
    };
}

/// Performs a string => object => string conversion and asserts that the final
/// string is the same as the input one.
///
/// This makes sure that both the [`Read`] and [`Write`] impls work correctly.
#[cfg(test)]
#[macro_export]
macro_rules! assert_trip {
    // Asserting components is tricky, because there's a discrepancy between
    // their reader and writer traits - `impl Read` expects that `END:...` is
    // part of the string, while `impl Write` doesn't print that on its own, so
    // a literal 1:1 assertion would fail.
    ($str:expr, $ty:ty as Component($name:literal)) => {
        let given = format!("{}\nEND:{}", $str, $name);

        pretty_assertions::assert_eq!(
            $str,
            <$ty as IcsRead<_>>::from_str(&given, Component)
                .unwrap()
                .to_string(Component)
        );
    };

    ($str:expr, $ty:ty as $marker:expr) => {
        assert_trip!($str => $str, $ty as $marker);
    };

    ($lhs:expr => $rhs:expr, $ty:ty as $marker:expr) => {
        assert_trip!($lhs => $rhs, yielding [], $ty as $marker);
    };

    ($lhs:expr => $rhs:expr, yielding $msgs:expr, $ty:ty as $marker:expr) => {
        let (actual_obj, actual_msgs) = <$ty as IcsRead<_>>::from_str_ex(&$lhs, $marker);

        pretty_assertions::assert_eq!(
            Vec::<ReadMsg>::from($msgs),
            actual_msgs,
        );

        pretty_assertions::assert_eq!(
            $rhs,
            actual_obj.unwrap().to_string($marker),
        );
    };
}

#[must_use]
#[track_caller]
#[doc(hidden)]
pub fn d(s: &str) -> Date {
    Date::from_str(s, Value).unwrap()
}

#[must_use]
#[track_caller]
#[doc(hidden)]
pub fn t(s: &str) -> Time {
    Time::from_str(s, Value).unwrap()
}

#[must_use]
#[track_caller]
#[doc(hidden)]
pub fn dt(s: &str) -> DateTime {
    if s.starts_with([':', ';']) {
        DateTime::from_str(s, Property).unwrap()
    } else {
        DateTime::from_str(&format!(":{s}"), Property).unwrap()
    }
}

#[must_use]
#[track_caller]
#[doc(hidden)]
pub fn dte<F>(s: &str) -> DateTime<F>
where
    DateTime<F>: IcsRead<Value>,
{
    DateTime::from_str(s, Value).unwrap()
}

#[must_use]
#[track_caller]
#[doc(hidden)]
pub fn dur(s: &str) -> Duration {
    if s.starts_with([':', ';']) {
        Duration::from_str(s, Property).unwrap()
    } else {
        Duration::from_str(s, Value).unwrap()
    }
}

#[must_use]
#[track_caller]
#[doc(hidden)]
pub fn recur(s: &str) -> Recur {
    Recur::from_str(s, Value).unwrap()
}

#[must_use]
#[track_caller]
#[doc(hidden)]
pub fn email(s: &str) -> EmailAddress {
    EmailAddress::from(s)
}

#[must_use]
#[track_caller]
#[doc(hidden)]
pub fn jz(s: &str) -> jiff::Zoned {
    if s.contains('[') {
        s.parse().unwrap()
    } else {
        format!("{s}[UTC]").parse().unwrap()
    }
}

#[must_use]
#[doc(hidden)]
pub fn cal() -> VCalendar {
    VCalendar::new("test")
}

#[must_use]
pub fn prodid() -> String {
    format!(
        "-//Proton AG//oxidized-calendar {}.{}.{}//EN",
        env!("CARGO_PKG_VERSION_MAJOR"),
        env!("CARGO_PKG_VERSION_MINOR"),
        env!("CARGO_PKG_VERSION_PATCH"),
    )
}
