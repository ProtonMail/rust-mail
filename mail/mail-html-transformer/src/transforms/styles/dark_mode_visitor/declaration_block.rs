use std::convert::Infallible;

use lightningcss::{
    printer::PrinterOptions,
    properties::Property,
    values::color::{CssColor, HSL},
    visit_types,
    visitor::{Visit, Visitor},
};

use crate::transforms::styles::{
    ColorPurpose, DARK_MODE_BACKGROUND_COLOR, NewProperty, OldProperty, PropertyWithPurpose,
    colors::{HSLExt, css_to_hsla},
};

use super::colors::ShouldModifyTransparentColors;
use super::properties::PropertiesVisitor;

/// Whether to keep serialized overriden props (as in original props before the edit)
/// in the visitor result.
/// Useful while parsing style attributes but useless when parsing stylesheets
#[derive(Default, Clone, Debug)]
pub enum ShouldStoreOverridenProps {
    Yes,
    #[default]
    No,
}

/// Whether to remove `!important` flag from the original properties.
///
/// When we are parsing stylesheets we do not need to remove `!important` flag. Instead
/// we can create a supplement with higher specificity.
///
/// However, when we are parsing `style=""` attribute, `!important` flag has utmost priority.
/// We need to remove it in order to be able to override the property.
///
#[derive(Clone, Copy, Debug, Default)]
pub enum ShouldRemoveImportant {
    Yes,
    #[default]
    No,
}

/// Walks through the CSS declaration block, detects which
/// color needs to be adjusted to dark theme.
/// It modifies original stylesheet by removing `!important` flag if necessary.
/// The result of the dark-mode theming is available under [`StylesheetVisitor::overrides`] method.
///
#[derive(Default, Clone, Debug)]
pub(crate) struct DeclarationBlockVisitor {
    overriden: Vec<OldProperty>,

    overrides: Vec<NewProperty>,

    should_store_overriden_props: ShouldStoreOverridenProps,

    should_remove_important: ShouldRemoveImportant,

    should_modify_transparent_colors: ShouldModifyTransparentColors,

    pub printer_options: PrinterOptions<'static>,
}

impl DeclarationBlockVisitor {
    pub fn new(
        should_store_overriden_props: ShouldStoreOverridenProps,
        should_remove_important: ShouldRemoveImportant,
        should_modify_transparent_colors: ShouldModifyTransparentColors,
        printer_options: PrinterOptions<'static>,
    ) -> Self {
        Self {
            should_store_overriden_props,
            should_remove_important,
            should_modify_transparent_colors,
            printer_options,
            ..Default::default()
        }
    }

    pub fn overrides(self) -> (Vec<OldProperty>, Vec<NewProperty>) {
        (self.overriden, self.overrides)
    }
}

impl Visitor<'_> for DeclarationBlockVisitor {
    type Error = Infallible;

    fn visit_types(&self) -> lightningcss::visitor::VisitTypes {
        visit_types!(RULES | PROPERTIES)
    }

    fn visit_declaration_block(
        &mut self,
        decls: &mut lightningcss::declaration::DeclarationBlock<'_>,
    ) -> Result<(), Self::Error> {
        let mut visitor = PropertiesVisitor::new(self.should_modify_transparent_colors);

        decls.visit_children(&mut visitor)?;

        if !visitor.modified.is_empty() {
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
                        .to_css_string(true, self.printer_options)
                        .inspect_err(|err| {
                            tracing::error!("Could not print CSS: {err:?}. Skipping it");
                        })
                        .ok()
                })
                .collect::<Vec<_>>();

            self.overrides.extend(overrides);

            for prop in visitor.modified {
                // to_css_string is potentially expensive operation
                if matches!(
                    self.should_store_overriden_props,
                    ShouldStoreOverridenProps::Yes
                ) {
                    match prop.to_css_string(false, self.printer_options) {
                        Ok(overriden_prop) => {
                            self.overriden.push(overriden_prop);
                        }
                        _ => {
                            tracing::error!("Could not print original CSS to string. Skipping it.");
                        }
                    }
                }

                if matches!(self.should_remove_important, ShouldRemoveImportant::Yes)
                    && let Some(pos) = decls.important_declarations.iter().position(|p| p == &prop)
                {
                    decls.important_declarations.remove(pos);
                    decls.declarations.push(prop);
                }
            }
        }
        Ok(())
    }
}

impl DeclarationBlockVisitor {
    fn has_good_contrast_against_backgrounds(
        bgs: &[PropertyWithPurpose<'_>],
        fg: &Property<'_>,
    ) -> bool {
        if bgs.is_empty() {
            return Self::has_good_contrast_against_color(DARK_MODE_BACKGROUND_COLOR.into(), fg);
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
            Self::extract_color_from_prop(bg).unwrap_or_else(|| DARK_MODE_BACKGROUND_COLOR.into()),
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
        let Ok(hsl) = css_to_hsla(color) else {
            return Ok(());
        };

        self.color = Some(hsl);

        Ok(())
    }
}
