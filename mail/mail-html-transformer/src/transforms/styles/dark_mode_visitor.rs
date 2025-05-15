use std::convert::Infallible;

use colors::HSLExt;
use lightningcss::{
    printer::PrinterOptions,
    properties::Property,
    rules::CssRule,
    traits::ToCss,
    values::color::{CssColor, HSL, RGBA},
    visit_types,
    visitor::{Visit, Visitor},
};
use properties::PropertiesVisitor;
use smart_default::SmartDefault;

use super::{
    ColorPurpose, PropertyWithPurpose, Selector, StyleOverrides, dark_mode_background_color,
};

mod colors;
mod properties;

/// Walks through the CSS stylesheet, detects which
/// color needs to be adjusted to dark theme.
/// It modifies original stylesheet by removing `!important` flag if necessary.
/// The result of the dark-mode theming is available under [`DarkModeVisitor::overrides`] method.
///
#[derive(SmartDefault, Clone)]
pub(crate) struct DarkModeVisitor {
    overrides: StyleOverrides,

    selector_stack: Vec<Selector>,

    // Because PrinterOptions do not implement Clone
    #[default(super::printer_options)]
    pub printer_options: fn() -> PrinterOptions<'static>,
}

impl DarkModeVisitor {
    pub fn new(printer_options: fn() -> PrinterOptions<'static>) -> Self {
        Self {
            printer_options,
            ..Default::default()
        }
    }

    pub fn overrides(self) -> StyleOverrides {
        self.overrides
            .into_iter()
            .filter(|(_key, values)| !values.is_empty())
            .collect()
    }
}

impl Visitor<'_> for DarkModeVisitor {
    type Error = Infallible;

    fn visit_types(&self) -> lightningcss::visitor::VisitTypes {
        visit_types!(RULES | PROPERTIES | COLORS)
    }

    fn visit_declaration_block(
        &mut self,
        decls: &mut lightningcss::declaration::DeclarationBlock<'_>,
    ) -> Result<(), Self::Error> {
        let mut visitor = PropertiesVisitor::new();

        decls.visit_children(&mut visitor)?;

        if !visitor.modified.is_empty() {
            let selectors = self.selector_stack.clone();

            let (bg_overrides, fg_overrides): (Vec<_>, Vec<_>) = visitor
                .overrides
                .into_iter()
                .partition(|prop| match prop.color_purpose {
                    ColorPurpose::Foreground => false,
                    ColorPurpose::Background => true,
                });

            let original_fg = visitor
                .modified
                .clone()
                .into_iter()
                .filter(PropertiesVisitor::is_foreground)
                .collect::<Vec<_>>();

            let fg = fg_overrides
                .into_iter()
                .zip(original_fg)
                .collect::<Vec<_>>();

            let fg_overrides = fg
                .into_iter()
                .filter_map(|(fg_override, original_fg)| {
                    if let Some(color) = Self::extract_color_from_prop(&original_fg) {
                        let rgba: RGBA = color.into();
                        if rgba == RGBA::transparent() {
                            return None;
                        }
                    }
                    if !Self::has_good_contrast_against_backgrounds(&bg_overrides, &original_fg) {
                        return Some(fg_override);
                    }

                    None
                })
                .collect::<Vec<_>>();

            let overrides = bg_overrides
                .into_iter()
                .chain(fg_overrides)
                .filter_map(|prop| {
                    prop.property
                        .to_css_string(true, (self.printer_options)())
                        .inspect_err(|err| {
                            tracing::error!("Could not print CSS: {err:?}. Skipping it");
                        })
                        .ok()
                })
                .collect::<Vec<_>>();

            self.overrides
                .entry(selectors)
                .or_default()
                .extend(overrides);

            for prop in visitor.modified {
                if let Some(pos) = decls.important_declarations.iter().position(|p| p == &prop) {
                    decls.important_declarations.remove(pos);
                    decls.declarations.push(prop);
                }
            }
        }
        Ok(())
    }

    fn visit_rule(&mut self, rule: &mut CssRule<'_>) -> Result<(), Self::Error> {
        let Some(selectors) = self.get_selectors(rule) else {
            // We either are processing non-style rule or a rule that has no printable selector.
            // The best we can do is to continue traversing the tree.
            rule.visit_children(self)?;
            return Ok(());
        };

        self.selector_stack.push(selectors);

        rule.visit_children(self)?;

        self.selector_stack.pop();

        Ok(())
    }
}

impl DarkModeVisitor {
    fn get_selectors(&self, rule: &CssRule<'_>) -> Option<String> {
        let printer_options = (self.printer_options)();
        match rule {
            CssRule::Style(style) => style.selectors.to_css_string(printer_options).ok(),
            CssRule::Media(media_rule) => {
                let query = media_rule.query.to_css_string(printer_options).ok();

                // If the media query always matches, we can just skip this selector.
                if (self.printer_options)().minify && media_rule.query.always_matches() {
                    return None;
                }

                query.map(|q| format!("@media {q}"))
            }
            _ => None,
        }
    }

    fn has_good_contrast_against_backgrounds(
        bgs: &[PropertyWithPurpose<'_>],
        fg: &Property<'_>,
    ) -> bool {
        if bgs.is_empty() {
            return Self::has_good_contrast_against_color(dark_mode_background_color().into(), fg);
        }

        bgs.iter()
            .all(|bg| Self::has_good_contrast_against_background(&bg.property, fg))
    }

    fn has_good_contrast_against_color(color: HSL, fg: &Property<'_>) -> bool {
        let bg_luminance = color.relative_luminance();

        // If the foreground does not specify the color we assume lightest white
        let fg_luminance =
            Self::extract_color_from_prop(fg).map_or(1.0, |hsl| hsl.relative_luminance());

        let lighter = fg_luminance.max(bg_luminance);
        let darker = fg_luminance.min(bg_luminance);

        let color_contrast_ratio = (lighter + 0.05) / (darker + 0.05);
        color_contrast_ratio >= 4.5
    }

    fn has_good_contrast_against_background(bg: &Property<'_>, fg: &Property<'_>) -> bool {
        Self::has_good_contrast_against_color(
            Self::extract_color_from_prop(bg)
                .unwrap_or_else(|| dark_mode_background_color().into()),
            fg,
        )
    }

    fn extract_color_from_prop(property: &Property<'_>) -> Option<HSL> {
        let mut visitor = HSLColorExtractor { color: None };

        // Infallible
        _ = property.clone().visit_children(&mut visitor);

        visitor.color
    }
}

/// Helper visitor that just extracts HSL from the property (if the property contains color and the color is
/// transformable to HSL colorspace).
/// No extra calculations are done
struct HSLColorExtractor {
    color: Option<HSL>,
}

impl Visitor<'_> for HSLColorExtractor {
    type Error = Infallible;

    fn visit_types(&self) -> lightningcss::visitor::VisitTypes {
        visit_types!(COLORS)
    }

    fn visit_color(&mut self, color: &mut CssColor) -> Result<(), Self::Error> {
        let Ok(hsl) = HSL::try_from(color.clone()) else {
            tracing::error!("Could not transform {color:?} into HSL colorspace. Skipping it");
            return Ok(());
        };

        self.color = Some(hsl);

        Ok(())
    }
}
