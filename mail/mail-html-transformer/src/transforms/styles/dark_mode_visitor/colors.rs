use std::convert::Infallible;

use lightningcss::{values::color::CssColor, visit_types, visitor::Visitor};

use crate::transforms::styles::{
    ColorPurpose,
    colors::{HSLExt, css_to_hsla, hsla_for_dark_mode},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ShouldModifyTransparentColors {
    Yes,
    #[default]
    No,
}

/// This visitor should be created per-property
pub(crate) struct ColorVisitor {
    /// Is the currently inspected property a background property or not.
    color_purpose: ColorPurpose,
    should_modify_transparent_colors: ShouldModifyTransparentColors,
}

impl ColorVisitor {
    pub(crate) fn new(
        color_purpose: ColorPurpose,
        should_modify_transparent_colors: ShouldModifyTransparentColors,
    ) -> Self {
        Self {
            color_purpose,
            should_modify_transparent_colors,
        }
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

        if hsl.is_transparent()
            && !matches!(
                self.should_modify_transparent_colors,
                ShouldModifyTransparentColors::Yes
            )
        {
            return Ok(());
        }

        *color = CssColor::RGBA(hsla_for_dark_mode(
            self.color_purpose,
            hsl,
            self.should_modify_transparent_colors,
        ));

        Ok(())
    }
}
