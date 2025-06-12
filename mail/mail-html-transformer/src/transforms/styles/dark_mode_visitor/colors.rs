use std::convert::Infallible;

use lightningcss::{values::color::CssColor, visit_types, visitor::Visitor};

use crate::transforms::styles::{
    ColorPurpose,
    colors::{HSLExt, css_to_hsla, hsla_for_dark_mode},
};

/// This visitor should be created per-property
pub(crate) struct ColorVisitor {
    /// Is the currently inspected property a background property or not.
    color_purpose: ColorPurpose,
}

impl ColorVisitor {
    pub(crate) fn new(color_purpose: ColorPurpose) -> Self {
        Self { color_purpose }
    }
}

impl Visitor<'_> for ColorVisitor {
    type Error = Infallible;

    fn visit_types(&self) -> lightningcss::visitor::VisitTypes {
        visit_types!(COLORS)
    }

    fn visit_color(&mut self, color: &mut CssColor) -> Result<(), Self::Error> {
        let Ok(hsl) = css_to_hsla(color) else {
            return Ok(());
        };

        if hsl.is_transparent() {
            return Ok(());
        }

        *color = CssColor::RGBA(hsla_for_dark_mode(self.color_purpose, hsl));

        Ok(())
    }
}
