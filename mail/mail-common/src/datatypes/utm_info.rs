use std::collections::BTreeSet;

#[derive(Clone, Debug)]
pub struct StrippedUTMInfo {
    pub links: BTreeSet<UTMLink>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct UTMLink {
    pub original_url: String,
    pub cleaned_url: String,
}
