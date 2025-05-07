use std::convert::Infallible;

use lightningcss::{
    values::color::{CssColor, HSL, RGBA, SRGBLinear, XYZd65},
    visit_types,
    visitor::Visitor,
};

use crate::transforms::styles::{ColorPurpose, dark_mode_background_color};

pub trait HSLExt {
    /// We want to see if our color is equivalent to `#FF_FF_FF_FF`
    fn is_full_white(&self) -> bool;
    fn is_achromatic(&self) -> IsColorAchromatic;

    /// How bright the color actually looks to a human eye
    /// Normalized as 0.0 - darkest black and 1.0 - lightest white
    fn relative_luminance(&self) -> f32;
}

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

        *color = CssColor::RGBA(hsla_for_dark_mode(self.color_purpose, hsl));

        Ok(())
    }
}

#[derive(Clone, Copy)]
pub enum IsColorAchromatic {
    Achromatic,
    Colorful,
}

/// Transform any color for given purpose to the dark mode equivalent
#[allow(clippy::enum_glob_use)]
fn hsla_for_dark_mode(purpose: ColorPurpose, mut color: HSL) -> RGBA {
    use IsColorAchromatic::*;

    // Special case: For full white background we return our Dark Mode background color (#1C1B24)
    if purpose == ColorPurpose::Background && color.is_full_white() {
        return dark_mode_background_color();
    }

    match (purpose, color.is_achromatic()) {
        (ColorPurpose::Foreground, Achromatic) => {
            return HSL {
                h: 0.0,
                s: 0.0,
                l: 1.0,
                alpha: color.alpha,
            }
            .into();
        }
        (ColorPurpose::Background, Achromatic) => {
            return HSL {
                h: 230.0,
                s: 0.12,
                l: 0.1,
                alpha: color.alpha,
            }
            .into();
        }
        (ColorPurpose::Foreground, Colorful) => {
            color.l = color.l.max(0.9);
        }
        (ColorPurpose::Background, Colorful) => {
            color.l = color.l.min(0.3);
        }
    }

    color.into()
}

impl HSLExt for HSL {
    fn is_full_white(&self) -> bool {
        // We skip Hue. It doesn't matter with full white.
        let HSL { h: _, s, l, alpha } = *self;

        if !eq_with_tolerance(s, 0.0) {
            return false;
        }

        if !eq_with_tolerance(l, 1.0) {
            return false;
        }

        eq_with_tolerance(alpha, 1.0)
    }

    fn is_achromatic(&self) -> IsColorAchromatic {
        if self.s <= 0.05 {
            IsColorAchromatic::Achromatic
        } else {
            IsColorAchromatic::Colorful
        }
    }

    fn relative_luminance(&self) -> f32 {
        // 1. transform HSLA into linear RGB color space (`SRGBLinear`)
        //    * RGB (and HSL) are gamma-corrected (non-linear)
        //    * linear standard RGB as name suggests removes gamma correction
        //    * It represents actual physical light properties, not RGB screen tricks.
        //
        // 2. transform `SRGBLinear` into `XYZd65` colorspace
        //    * Since we need the relative luminance - as how bright the color actually looks to a human eye.
        //    * XYZd65 is composed from matrix:
        //
        //      0.4123908, 0.3575843, 0.1804808,
        //      0.2126390, 0.7151687, 0.0721923, <--- Y component. Those weights are precisely the same as constants
        //      0.0193308, 0.1191948, 0.9505322         used in WCAG's definition of relative luminance
        //                                              See: <https://www.w3.org/TR/WCAG20/#relativeluminancedef>
        // 3. Retrieve that Y component

        let lin: SRGBLinear = (*self).into();
        let xyz: XYZd65 = lin.into();
        xyz.y
    }
}

/// Comparing floats is a bad idea. Instead we need to have some acceptable
/// margin of float error. It cannot be EPSILON (See: <https://github.com/rust-lang/rust-clippy/pull/13079>)
/// but in case of comparing colors 0.1 % is rather acceptable value
fn eq_with_tolerance(x: f32, y: f32) -> bool {
    (x - y).abs() < 0.001
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
