//! Lean text token handling

use core::hash::Hash;
use core::ops::Deref;

use crate::prelude::*;

/// A structure that fits at most three unicode characters.
///
/// The benefit of it is being smaler than Box<str> and that it is heapless
///
/// One can get an &str from it with deref `&*token`
#[derive(Clone)]
pub enum Token {
    /// Token on the heap
    Boxed(Box<str>),
    /// Token in an array
    Local(u8, [u8; Self::LOCAL_SIZE as usize]),
}

impl Default for Token {
    fn default() -> Self {
        Self::Local(0, [0; Self::LOCAL_SIZE as usize])
    }
}

impl Token {
    const LOCAL_SIZE: u8 = 30;

    /// Create a new token from the head of the provided str.
    ///
    /// If the given str is correct, the token is also a correct utf-8 str.
    ///
    /// Panics: if the token is longer than 255 bytes.
    pub fn new(text: impl AsRef<str>) -> Self {
        let text = text.as_ref();

        if text.len() > Self::LOCAL_SIZE as usize {
            Self::Boxed(text.into())
        } else {
            let mut local = [0; Self::LOCAL_SIZE as usize];
            let len = text.len();
            #[allow(clippy::expect_used, reason = "impossible due to lenght check")]
            let length = u8::try_from(len).expect("too long a token");
            local[0..len].copy_from_slice(&text.as_bytes()[0..len]);
            Self::Local(length, local)
        }
    }

    /// Get the token length
    /// This function guarrantees a length of valid utf-8 bytes in the buffer
    pub fn len(&self) -> usize {
        match self {
            Token::Boxed(s) => s.len(),
            Token::Local(length, _bytes) => *length as usize,
        }
    }

    /// Check if the token is empty
    #[allow(unused)]
    pub fn is_empty(&self) -> bool {
        match self {
            Token::Boxed(s) => s.is_empty(),
            Token::Local(length, _bytes) => *length == 0,
        }
    }
}

impl Ord for Token {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.as_ref().cmp(other.as_ref())
    }
}

impl<T: AsRef<str>> PartialOrd<T> for Token {
    fn partial_cmp(&self, other: &T) -> Option<core::cmp::Ordering> {
        Some(self.deref().cmp(other.as_ref()))
    }
}

impl Eq for Token {}

impl<T: AsRef<str>> PartialEq<T> for Token {
    fn eq(&self, other: &T) -> bool {
        self.deref().eq(other.as_ref())
    }
}

impl Hash for Token {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.as_ref().hash(state);
    }
}

impl AsRef<str> for Token {
    fn as_ref(&self) -> &str {
        self
    }
}

impl Deref for Token {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            Token::Boxed(s) => s.as_ref(),
            Token::Local(length, bytes) => {
                #[allow(
                    unsafe_code,
                    reason = "It is guarranteed to be valid utf-8 through the len() fn"
                )]
                let token = unsafe { str::from_utf8_unchecked(&bytes[0..*length as usize]) };
                token
            }
        }
    }
}

impl From<&str> for Token {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}
impl From<String> for Token {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
impl From<&String> for Token {
    fn from(value: &String) -> Self {
        Self::new(value)
    }
}
impl From<Box<str>> for Token {
    fn from(value: Box<str>) -> Self {
        Self::new(value)
    }
}
impl From<&Box<str>> for Token {
    fn from(value: &Box<str>) -> Self {
        Self::new(value)
    }
}
impl From<alloc::borrow::Cow<'_, str>> for Token {
    fn from(value: alloc::borrow::Cow<'_, str>) -> Self {
        Self::new(value)
    }
}
impl From<&'_ alloc::borrow::Cow<'_, str>> for Token {
    fn from(value: &'_ alloc::borrow::Cow<'_, str>) -> Self {
        Self::new(value)
    }
}

impl From<Token> for Box<str> {
    fn from(value: Token) -> Self {
        match value {
            Token::Boxed(b) => b.clone(),
            Token::Local(..) => value.as_ref().into(),
        }
    }
}

impl<'a> From<&'a Token> for Box<str> {
    fn from(value: &'a Token) -> Self {
        match value {
            Token::Boxed(b) => b.clone(),
            Token::Local(..) => value.as_ref().into(),
        }
    }
}

impl core::fmt::Display for Token {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl core::fmt::Debug for Token {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Deref::deref(self).fmt(f)
    }
}

impl serde::Serialize for Token {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.deref().serialize(serializer)
    }
}
impl<'de> serde::Deserialize<'de> for Token {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let string: Box<str> = serde::Deserialize::deserialize(deserializer)?;
        Ok(Self::new(string))
    }
}
impl core::borrow::Borrow<str> for Token {
    fn borrow(&self) -> &str {
        self
    }
}

#[test]
fn token_extraction() {
    // String - long
    let token: Token = String::from("abcdef______________________________123456778987").into();
    assert_eq!(&*token, "abcdef______________________________123456778987");
    assert_eq!(token.len(), 48);
    assert!(matches!(token, Token::Boxed(_)));

    // String - longer
    let token: Token = Box::<str>::from("abcdef").into();
    assert_eq!(&*token, "abcdef");
    assert_eq!(token.len(), 6);
    assert!(matches!(token, Token::Local(6, _)));

    // Box<str> - longer unicode
    let token: Token = "ščřžě".into();
    assert_eq!(&*token, "ščřžě");
    assert_eq!(token.len(), 10);

    // &str - shorter unicode
    let token: Token = "š".into();
    assert_eq!(&*token, "š");
    assert_eq!(token.len(), 2);

    // &str - empty
    let token: Token = "".into();
    assert_eq!(&*token, "");
    assert_eq!(token.len(), 0);
}

#[test]
fn meaningful_size() {
    assert_eq!(size_of::<Token>(), 32);
    assert_eq!(size_of::<Box<str>>(), 16);
}

#[test]
fn deref_trick() {
    let token: Token = "abc".into();
    assert_eq!(&*token, "abc");
}

#[test]
fn as_ref_works() {
    fn takes_as_ref(_: impl AsRef<str>) {}
    let token: Token = "abc".into();
    takes_as_ref(&token);
    takes_as_ref(token);
}

#[test]
fn deref_works() {
    fn takes_deref(_: impl Deref<Target = str>) {}
    let token: Token = "abc".into();
    takes_deref(token);
}

#[test]
fn sort_order() {
    let mut strings = vec!["", "a", "alt", "š", "s", "šál", "\0"];
    let mut tokens = strings.iter().copied().map(Token::from).collect::<Vec<_>>();

    strings.sort();
    tokens.sort();
    assert_eq!(
        tokens.iter().map(|t| t.deref()).collect::<Vec<_>>(),
        strings
    );
}

#[test]
fn empty_is_not_utf8() {
    let test = b"\x80other";
    #[allow(invalid_from_utf8)]
    let result = core::str::from_utf8(test);
    assert!(result.is_err())
}
