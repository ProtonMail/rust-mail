use std::convert::Infallible;

use lightningcss::{
    printer::PrinterOptions,
    rules::CssRule,
    traits::ToCss,
    visit_types,
    visitor::{Visit, Visitor},
};

use crate::transforms::styles::{
    Selector, StylesheetOverrides, dark_mode_visitor::declaration_block::ShouldRemoveImportant,
};

use super::colors::ShouldModifyTransparentColors;
use super::declaration_block::{DeclarationBlockVisitor, ShouldStoreOverriddenProps};

/// Walks through the CSS stylesheet, detects which
/// color needs to be adjusted to dark theme.
/// It modifies original stylesheet by removing `!important` flag if necessary.
/// The result of the dark-mode theming is available under [`StylesheetVisitor::overrides`] method.
///
#[derive(Default, Clone, Debug)]
pub(crate) struct StylesheetVisitor {
    overrides: StylesheetOverrides,

    selector_stack: Vec<Selector>,

    root_selector: String,

    current_selector_is_body_or_html: bool,

    pub printer_options: PrinterOptions<'static>,
}
impl StylesheetVisitor {
    pub fn new(root_selector: String) -> Self {
        Self {
            root_selector,
            ..Default::default()
        }
    }

    pub fn overrides(self) -> StylesheetOverrides {
        self.overrides
            .into_iter()
            .filter(|(_key, values)| !values.is_empty())
            .collect()
    }
}

impl Visitor<'_> for StylesheetVisitor {
    type Error = Infallible;

    fn visit_types(&self) -> lightningcss::visitor::VisitTypes {
        visit_types!(RULES | PROPERTIES)
    }

    fn visit_declaration_block(
        &mut self,
        decls: &mut lightningcss::declaration::DeclarationBlock<'_>,
    ) -> Result<(), Self::Error> {
        let should_modify_transparent_colors = if self.current_selector_is_body_or_html {
            ShouldModifyTransparentColors::Yes
        } else {
            ShouldModifyTransparentColors::No
        };

        let mut visitor = DeclarationBlockVisitor::new(
            ShouldStoreOverriddenProps::No,
            ShouldRemoveImportant::No,
            should_modify_transparent_colors,
            self.printer_options,
        );

        decls.visit(&mut visitor)?;

        let (_, overrides) = visitor.overrides();
        if !overrides.is_empty() {
            let selectors = self.selector_stack.clone();
            self.overrides
                .entry(selectors)
                .or_default()
                .extend(overrides);
        }

        Ok(())
    }

    fn visit_rule(&mut self, rule: &mut CssRule<'_>) -> Result<(), Self::Error> {
        let Some((selectors, is_body_or_html)) = self.get_selectors(rule) else {
            // We either are processing non-style rule or a rule that has no printable selector.
            // The best we can do is to continue traversing the tree.
            rule.visit_children(self)?;
            return Ok(());
        };

        self.selector_stack.push(selectors);
        let previous_is_body_or_html = self.current_selector_is_body_or_html;
        self.current_selector_is_body_or_html = is_body_or_html;

        rule.visit_children(self)?;

        self.selector_stack.pop();
        self.current_selector_is_body_or_html = previous_is_body_or_html;

        Ok(())
    }
}

impl StylesheetVisitor {
    fn get_selectors(&self, rule: &CssRule<'_>) -> Option<(String, bool)> {
        let printer_options = self.printer_options;
        match rule {
            CssRule::Style(style) => {
                style
                    .selectors
                    .to_css_string(printer_options)
                    .ok()
                    .map(|selector| {
                        let is_body_or_html = selector == "html" || selector == "body";
                        let formatted_selector = if selector == "html" {
                            format!("html{}", self.root_selector)
                        } else {
                            format!("{} {}", self.root_selector, selector)
                        };
                        (formatted_selector, is_body_or_html)
                    })
            }
            CssRule::Media(media_rule) => {
                let query = media_rule.query.to_css_string(printer_options).ok();

                // If the media query always matches, we can just skip this selector.
                if printer_options.minify && media_rule.query.always_matches() {
                    return None;
                }

                query.map(|q| (format!("@media {q}"), false))
            }
            _ => None,
        }
    }
}
