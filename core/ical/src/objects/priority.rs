/// Priority.
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.8.1.9>
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Priority {
    value: u8,
}

impl Priority {
    /// Creates a new priority.
    ///
    /// 0 represents an undefined priority, 1 represents the highest priority,
    /// and the lowest priority is 9 (i.e. value passed here must be <= 9).
    #[must_use]
    pub fn new(value: u8) -> Option<Self> {
        if value <= 9 {
            Some(Self { value })
        } else {
            None
        }
    }

    #[must_use]
    pub fn new_unchecked(value: u8) -> Self {
        Self { value }
    }

    #[must_use]
    pub fn as_num(&self) -> u8 {
        self.value
    }
}

impl From<u32> for Priority {
    fn from(value: u32) -> Self {
        Priority::new_unchecked(value.min(9) as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constructors() {
        for value in 0..=9 {
            assert_eq!(value, Priority::new(value).unwrap().value);
        }

        for value in 0..=9 {
            assert_eq!(value, Priority::from(u32::from(value)).value);
        }

        assert_eq!(None, Priority::new(10));
        assert_eq!(9, Priority::from(10).value);
    }
}
