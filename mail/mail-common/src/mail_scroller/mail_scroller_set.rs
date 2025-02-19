use std::ops::Deref;

/// A set of mail items that MailScroller returns
///
/// They indicate if items can be appended or have to be replaced.
///
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MailScrollerSet<T> {
    Append(Vec<T>),
    Replace(Vec<T>),
}

impl<T> Deref for MailScrollerSet<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        match self {
            MailScrollerSet::Append(v) => v,
            MailScrollerSet::Replace(v) => v,
        }
    }
}

impl<T> IntoIterator for MailScrollerSet<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            MailScrollerSet::Append(v) => v.into_iter(),
            MailScrollerSet::Replace(v) => v.into_iter(),
        }
    }
}

impl<T> From<MailScrollerSet<T>> for Vec<T> {
    fn from(set: MailScrollerSet<T>) -> Self {
        match set {
            MailScrollerSet::Append(v) => v,
            MailScrollerSet::Replace(v) => v,
        }
    }
}

#[cfg(any(test, debug_assertions))]
impl<T: PartialEq> PartialEq<Vec<T>> for MailScrollerSet<T> {
    fn eq(&self, other: &Vec<T>) -> bool {
        self.deref() == other
    }
}

#[cfg(any(test, debug_assertions))]
impl<T: PartialEq> PartialEq<MailScrollerSet<T>> for Vec<T> {
    fn eq(&self, other: &MailScrollerSet<T>) -> bool {
        self == other.deref()
    }
}
