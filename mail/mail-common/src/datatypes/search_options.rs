#[derive(Clone, Debug, Default)]
pub struct SearchOptions {
    pub keywords: Option<String>,
}

impl<A: AsRef<str>> From<A> for SearchOptions {
    fn from(value: A) -> Self {
        Self {
            keywords: Some(value.as_ref().to_string()),
        }
    }
}
