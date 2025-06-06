use lightningcss::{
    traits::Parse,
    values::color::{CssColor, HSL, RGBA, SRGBLinear, XYZd65},
};

use crate::transforms::styles::{ColorPurpose, dark_mode_background_color};

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

        if !eq_with_tolerance(l, 1.0) {
            return false;
        }

        eq_with_tolerance(alpha, 1.0)
    }

    fn is_transparent(&self) -> bool {
        eq_with_tolerance(self.alpha, 0.0)
    }

    fn is_achromatic(&self) -> IsColorAchromatic {
        if self.s <= 0.06 {
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
pub fn hsla_for_dark_mode(purpose: ColorPurpose, mut color: HSL) -> RGBA {
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

pub fn parse_css_color(color: &str) -> Option<CssColor> {
    // TODO(wpolak): Create an issue in lightningcss to support hex colors without `#` prefix
    match CssColor::parse_string(color) {
        Ok(color) => Some(color),
        Err(err) => {
            // Lightningcss does not support hex colors without `#` prefix
            // Let's try to add it manually, if it works, we return the color
            let new_color = format!("#{color}");
            CssColor::parse_string(&new_color)
                .inspect_err(|_| {
                    // Let's display the original error message.
                    tracing::warn!("Could not parse color: {color}. Error: {err:?}");
                })
                .ok()
        }
    }
}
