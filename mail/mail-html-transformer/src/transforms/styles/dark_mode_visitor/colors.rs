use std::convert::Infallible;

use lightningcss::{
    values::color::{CssColor, HSL},
    visit_types,
    visitor::Visitor,
};

use crate::transforms::styles::{
    ColorPurpose,
    colors::{HSLExt, hsla_for_dark_mode},
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
        let Ok(hsl) = HSL::try_from(color.clone()) else {
            tracing::error!("Could not transform {color:?} into HSL colorspace. Skipping it");
            return Ok(());
        };

        if hsl.is_transparent() {
            return Ok(());
        }

        *color = CssColor::RGBA(hsla_for_dark_mode(self.color_purpose, hsl));

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightningcss::{
        printer::PrinterOptions,
        traits::{Parse, ToCss},
    };
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    // Test cases taken from ios Legacy:
    // https://gitlab.protontech.ch/ProtonMail/protonmail-ios/-/blob/995cc63d5d689cf120fca42139fd36dfd9fad975/ProtonMail/ProtonMailTests/ProtonMail/Utilities/DarkModeHelper/CSSMagicTest.swift#L556
    #[test_case(
        "hsla(0, 0%, 50%, 0.8)",
        ColorPurpose::Foreground,
        "#fffc"; // hsla(0, 0%, 100%, .8)
        "case 1"
    )]
    #[test_case(
        "hsla(0, 0%, 50%, 0.7)",
        ColorPurpose::Background,
        "#16171db3"; // hsla(230, 12%, 10%, .7)
        "case 2"
    )]
    #[test_case(
        "hsla(20, 50%, 70%, 1.0)",
        ColorPurpose::Foreground,
        "#f2e1d9"; // hsla(20, 50%, 90%, 1)
        "case 3"
    )]
    #[test_case(
        "hsla(20, 50%, 30%, 1.0)",
        ColorPurpose::Foreground,
        "#f2e1d9"; // hsla(20, 50%, 90%, 1)
        "case 4"
    )]
    #[test_case(
        "hsla(20, 50%, 3%, 1.0)",
        ColorPurpose::Background,
        "#0b0604"; // hsla(20, 50%, 3%, 1)
        "case 5"
    )]
    #[test_case(
        "hsla(20, 50%, 93%, 1.0)",
        ColorPurpose::Background,
        "#734026"; // hsla(20, 50%, 30%, 1)
        "case 6"
    )]
    #[test_case(
        "hsla(0, 0%, 100%, 1.0)",
        ColorPurpose::Background,
        "#1c1b24"; // Our Dark mode background color. Hardcoded value
        "case 7"
    )]
    fn hsla_for_dark_mode(input: &'static str, purpose: ColorPurpose, expected: &'static str) {
        let color = CssColor::parse_string(input).unwrap();
        let hsl = HSL::try_from(color).unwrap();

        let new_rgba = super::hsla_for_dark_mode(purpose, hsl);

        // LightningCSS rightfully converts HSLA format to RGBA.
        // Those two color spaces are equivalent without loss of information
        // and RGBA is shorter.
        // We took original expected values, converted them manually to RGBA and expect exactly that

        let new_color = CssColor::RGBA(new_rgba)
            .to_css_string(PrinterOptions::default())
            .unwrap();
        assert_eq!(expected, new_color);
    }
}
