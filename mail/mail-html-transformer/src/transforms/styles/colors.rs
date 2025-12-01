use lightningcss::values::color::{CssColor, HSL, RGBA, SRGBLinear, XYZd65};

use crate::transforms::styles::{ColorPurpose, DARK_MODE_BACKGROUND_COLOR};

use super::dark_mode_visitor::ShouldModifyTransparentColors;

pub trait HSLExt {
    /// We want to see if our color is equivalent to `#FF_FF_FF_FF`
    fn is_full_white(&self) -> bool;

    fn is_transparent(&self) -> bool;

    fn is_achromatic(&self) -> IsColorAchromatic;

    /// How bright the color actually looks to a human eye
    /// Normalized as 0.0 - darkest black and 1.0 - lightest white
    fn relative_luminance(&self) -> f32;
}

#[derive(Clone, Copy)]
pub enum IsColorAchromatic {
    Achromatic,
    Colorful,
}

/// Comparing floats is a bad idea. Instead we need to have some acceptable
/// margin of float error. It cannot be EPSILON (See: <https://github.com/rust-lang/rust-clippy/pull/13079>)
/// but in case of comparing colors 0.1 % is rather acceptable value
fn eq_with_tolerance(x: f32, y: f32) -> bool {
    (x - y).abs() < 0.001
}

impl HSLExt for HSL {
    fn is_full_white(&self) -> bool {
        // We skip Hue. It doesn't matter with full white.
        let HSL { h: _, s, l, alpha } = *self;

        if !eq_with_tolerance(s, 0.0) {
            return false;
        }

        if !eq_with_tolerance(l, 100.0) {
            return false;
        }

        eq_with_tolerance(alpha, 1.0)
    }

    fn is_transparent(&self) -> bool {
        eq_with_tolerance(self.alpha, 0.0)
    }

    fn is_achromatic(&self) -> IsColorAchromatic {
        if self.s <= 5.0 {
            IsColorAchromatic::Achromatic
        } else if self.l <= 1.0 {
            // We keep 1% as a threshold for dark colors in order to pass Test Case 5 (ported from legacy app).
            //
            // We want to treat very dark colors as achromatic.
            IsColorAchromatic::Achromatic
        } else if self.l >= 95.0 {
            // At the same time we are not using 99% as a threshold for light colors because
            // we found some background colors in the wild that for a human eye are light gray,
            // but in HSL representation are colorful.
            // For example:
            // * #FFFFFE - looks like white but HSL is (60, 100%, 100%) - without that if branch
            // we would transform it to ugly mustard color ( #999900 ).
            // We want to treat very light colors as achromatic.
            // * #FAF7F2 - looks like white but HSL is (38, 44%, 96%) - if we used 99% as a threshold
            // we would transform it to ugly orange color ( #705527 ).
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

/// Transform any color for given purpose to the dark mode equivalent
#[allow(clippy::enum_glob_use)]
pub fn hsla_for_dark_mode(
    purpose: ColorPurpose,
    mut color: HSL,
    should_modify_transparent: ShouldModifyTransparentColors,
) -> RGBA {
    use IsColorAchromatic::*;

    // Special case: For transparent background on body/html we return our Dark Mode background color
    if purpose == ColorPurpose::Background
        && color.is_transparent()
        && should_modify_transparent == ShouldModifyTransparentColors::Yes
    {
        return DARK_MODE_BACKGROUND_COLOR;
    }

    if color.is_transparent() {
        return color.into();
    }

    // Special case: For full white background we return our Dark Mode background color (#1C1B24)
    if purpose == ColorPurpose::Background && color.is_full_white() {
        return DARK_MODE_BACKGROUND_COLOR;
    }

    match (purpose, color.is_achromatic()) {
        (ColorPurpose::Foreground, Achromatic) => {
            return HSL {
                h: 0.0,
                s: 0.0,
                l: 100.0,
                alpha: color.alpha,
            }
            .into();
        }
        (ColorPurpose::Background, Achromatic) => {
            return HSL {
                h: 230.0,
                s: 12.0,
                l: 10.0,
                alpha: color.alpha,
            }
            .into();
        }
        (ColorPurpose::Foreground, Colorful) => {
            color.l = color.l.max(90.0);
        }
        (ColorPurpose::Background, Colorful) => {
            color.l = color.l.min(30.0);
        }
    }

    color.into()
}

pub fn css_to_hsla(color: &CssColor) -> Result<HSL, ()> {
    match color {
        CssColor::CurrentColor => {
            // `currentColor` is a special value that might be treated as a "pointer" to another property.
            // We do not need to support it, because we can assume, that our CSS injection will actually process the property
            // that `currentColor` points to.
            // Therefore here, we return `Err(())` that will be handled by the caller by skipping the property with it.
            Err(())
        }
        CssColor::LightDark(_light, dark) => {
            // `light-dark` allows CSS developer to define two colors, depending on the light or dark color theme.
            // Since we are transforming colors for dark mode, we can just use the dark color and adjust the contrast if necessary.
            css_to_hsla(dark)
        }
        CssColor::System(system) => {
            // We do not support system colors as these depend on the browser.
            tracing::debug!("System color ({:?}) is not supported", system);
            Err(())
        }
        CssColor::RGBA(rgba) => Ok((*rgba).into()),
        CssColor::LAB(lab) => Ok((**lab).into()),
        CssColor::Predefined(predefined) => Ok((**predefined).into()),
        CssColor::Float(float_color) => Ok((**float_color).into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lightningcss::{
        printer::PrinterOptions,
        traits::{Parse, ToCss},
        values::color::CssColor,
    };
    use pretty_assertions::assert_eq;
    use test_case::test_case;

    // Test cases taken from ios Legacy:
    // https://gitlab.protontech.ch/ProtonMail/protonmail-ios/-/blob/995cc63d5d689cf120fca42139fd36dfd9fad975/ProtonMail/ProtonMailTests/ProtonMail/Utilities/DarkModeHelper/CSSMagicTest.swift#L556
    #[test_case(
        "hsla(0, 0%, 50%, 0.8)",
        ColorPurpose::Foreground,
        ShouldModifyTransparentColors::No,
        "#fffc"; // hsla(0, 0%, 100%, .8)
        "case 1"
    )]
    #[test_case(
        "hsla(0, 0%, 50%, 0.7)",
        ColorPurpose::Background,
        ShouldModifyTransparentColors::No,
        "#16171db3"; // hsla(230, 12%, 10%, .7)
        "case 2"
    )]
    #[test_case(
        "hsla(20, 50%, 70%, 1.0)",
        ColorPurpose::Foreground,
        ShouldModifyTransparentColors::No,
        "#f2e1d9"; // hsla(20, 50%, 90%, 1)
        "case 3"
    )]
    #[test_case(
        "hsla(20, 50%, 30%, 1.0)",
        ColorPurpose::Foreground,
        ShouldModifyTransparentColors::No,
        "#f2e1d9"; // hsla(20, 50%, 90%, 1)
        "case 4"
    )]
    #[test_case(
        "hsla(20, 50%, 3%, 1.0)",
        ColorPurpose::Background,
        ShouldModifyTransparentColors::No,
        "#0b0604"; // hsla(20, 50%, 3%, 1)
        "case 5"
    )]
    #[test_case(
        "hsla(20, 50%, 93%, 1.0)",
        ColorPurpose::Background,
        ShouldModifyTransparentColors::No,
        "#734026"; // hsla(20, 50%, 30%, 1)
        "case 6"
    )]
    #[test_case(
        "hsla(0, 0%, 100%, 1.0)",
        ColorPurpose::Background,
        ShouldModifyTransparentColors::No,
        "#191927"; // Our Dark mode background color. Hardcoded value
        "case 7"
    )]
    #[test_case(
        "#fffffe",
        ColorPurpose::Background,
        ShouldModifyTransparentColors::No,
        "#16171d";
        "case 8"
    )]
    #[test_case(
        "#0000",
        ColorPurpose::Background,
        ShouldModifyTransparentColors::No,
        "#0000";
        "case 9"
    )]
    #[test_case(
        "#0000",
        ColorPurpose::Background,
        ShouldModifyTransparentColors::Yes,
        "#191927";
        "case 10"
    )]
    #[test_case(
        "#FFF0",
        ColorPurpose::Background,
        ShouldModifyTransparentColors::Yes,
        "#191927";
        "case 11"
    )]
    fn hsla_for_dark_mode(
        input: &'static str,
        purpose: ColorPurpose,
        should_modify_transparent: ShouldModifyTransparentColors,
        expected: &'static str,
    ) {
        let color = CssColor::parse_string(input).unwrap();
        let hsl = HSL::try_from(color).unwrap();

        let new_rgba = super::hsla_for_dark_mode(purpose, hsl, should_modify_transparent);

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
