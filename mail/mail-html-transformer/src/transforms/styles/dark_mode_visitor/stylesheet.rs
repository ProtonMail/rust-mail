use std::convert::Infallible;

use lightningcss::{
    printer::PrinterOptions,
    rules::CssRule,
    traits::ToCss,
    visit_types,
    visitor::{Visit, Visitor},
};
use smart_default::SmartDefault;

use crate::transforms::styles::{Selector, StylesheetOverrides, printer_options};

use super::declaration_block::{DeclarationBlockVisitor, ShouldStoreOverridenProps};

/// Walks through the CSS stylesheet, detects which
/// color needs to be adjusted to dark theme.
/// It modifies original stylesheet by removing `!important` flag if necessary.
/// The result of the dark-mode theming is available under [`StylesheetVisitor::overrides`] method.
///
#[derive(SmartDefault, Clone, Debug)]
pub(crate) struct StylesheetVisitor {
    overrides: StylesheetOverrides,

    selector_stack: Vec<Selector>,

    // Because PrinterOptions do not implement Clone
    #[default(printer_options)]
    pub printer_options: fn() -> PrinterOptions<'static>,
}
impl StylesheetVisitor {
    pub fn new(printer_options: fn() -> PrinterOptions<'static>) -> Self {
        Self {
            printer_options,
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
        let mut visitor =
            DeclarationBlockVisitor::new(ShouldStoreOverridenProps::No, self.printer_options);

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

impl StylesheetVisitor {
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
}
