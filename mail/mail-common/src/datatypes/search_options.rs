/// Object abstracting search options.
#[derive(Clone, Debug, Default)]
pub struct SearchOptions {
    /// Keywords is (possibly) multi word string which is passed
    /// unchanged to the Proton API.
    pub keywords: Option<String>,
}

impl<A: Into<String>> From<A> for SearchOptions {
    fn from(value: A) -> Self {
        Self {
            keywords: Some(value.into()),
        }
    }
}
