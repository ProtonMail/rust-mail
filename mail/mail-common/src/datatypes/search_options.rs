#[derive(Clone, Debug, Default)]
pub struct SearchOptions {
    pub keywords: Option<String>,
}

impl<A: Into<String>> From<A> for SearchOptions {
    fn from(value: A) -> Self {
        Self {
            keywords: Some(value.into()),
        }
    }
}
