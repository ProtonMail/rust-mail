use super::*;

/// Repeat count.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.6.2>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "php", derive(ZvalConvert))]
pub struct Repeat {
    pub value: u32,
}

impl From<u32> for Repeat {
    fn from(value: u32) -> Self {
        Self { value }
    }
}

impl Read<Property> for Repeat {
    fn read(r: &mut Reader) -> Option<Self> {
        r.burn_params();
        r.eat(':')?;

        Some(Self { value: r.value()? })
    }
}

impl Write<Property> for Repeat {
    fn write(&self, w: &mut Writer) {
        w.raw(":");
        w.value(self.value);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke() {
        assert_trip!(":123", Repeat as Property);
    }
}
