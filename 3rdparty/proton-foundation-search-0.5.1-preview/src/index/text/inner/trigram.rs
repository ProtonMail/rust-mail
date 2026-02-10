use std::ops::Deref;

use crate::index::text::trigram::Chars;

/// A structure that fits at most three unicode characters.
///
/// The benefit of it is being smaler than Box<str> and that it is heapless
///
/// One can get an &str from it with deref `&*trigram`
#[derive(Default, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Trigram {
    bytes: [u8; 12],
    len: u8,
}

impl Trigram {
    /// Create a new trigram from the head of the provided str.
    ///
    /// If the given str is correct, the trigram is also a correct utf-8 str.
    pub fn new(text: &str) -> Self {
        let (trigram, _rest) = text.split_chars(3);
        let trigram = trigram.as_bytes();

        let mut bytes = [0; 12];
        debug_assert!(trigram.len() <= bytes.len());
        let len = trigram.len().min(bytes.len());
        bytes[0..len].copy_from_slice(&trigram[0..len]);
        let len = len as u8;
        Self { bytes, len }
    }
}

impl Deref for Trigram {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        let Self { bytes, len } = self;
        match str::from_utf8(&bytes[0..*len as usize]) {
            Ok(trigram) => trigram,
            Err(e) => {
                // We could optimize with from_utf8_unchecked here
                unreachable!("Trigram must always be a valid utf-8 string - {e}")
            }
        }
    }
}

impl<T> From<T> for Trigram
where
    T: AsRef<str>,
{
    fn from(value: T) -> Self {
        Self::new(value.as_ref())
    }
}

impl std::fmt::Display for Trigram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl std::fmt::Debug for Trigram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        Deref::deref(self).fmt(f)
    }
}

impl serde::Serialize for Trigram {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.deref().serialize(serializer)
    }
}
impl<'de> serde::Deserialize<'de> for Trigram {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        Ok(Self::new(&String::deserialize(deserializer)?))
    }
}
impl std::borrow::Borrow<str> for Trigram {
    fn borrow(&self) -> &str {
        self
    }
}

#[test]
fn trigram_extraction() {
    // String - longer
    let trigram: Trigram = "abcdef".to_owned().into();
    assert_eq!(&*trigram, "abc");
    assert_eq!(trigram.len(), 3);

    // Box<str> - longer unicode
    let trigram: Trigram = "ščřžě".to_owned().into_boxed_str().into();
    assert_eq!(&*trigram, "ščř");
    assert_eq!(trigram.len(), 6);

    // &str - shorter unicode
    let trigram: Trigram = "š".into();
    assert_eq!(&*trigram, "š");
    assert_eq!(trigram.len(), 2);

    // &str - empty
    let trigram: Trigram = "".into();
    assert_eq!(&*trigram, "");
    assert_eq!(trigram.len(), 0);
}

#[test]
fn meaningful_size() {
    assert_eq!(size_of::<Trigram>(), 13);
    assert_eq!(size_of::<Box<str>>(), 16);
}

#[test]
fn deref_trick() {
    let trigram: Trigram = "abc".into();
    assert_eq!(&*trigram, "abc");
}

#[test]
fn sort_order() {
    let mut strings = vec!["", "a", "alt", "š", "s", "šál", "\0"];
    let mut trigrams = strings.iter().map(Trigram::from).collect::<Vec<_>>();

    strings.sort();
    trigrams.sort();
    assert_eq!(
        trigrams.iter().map(|t| t.deref()).collect::<Vec<_>>(),
        strings
    );
}
