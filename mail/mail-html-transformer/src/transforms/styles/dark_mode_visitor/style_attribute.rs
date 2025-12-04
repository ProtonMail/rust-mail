use std::convert::Infallible;

use lightningcss::{
    printer::PrinterOptions,
    visit_types,
    visitor::{Visit, Visitor},
};

use crate::transforms::styles::{
    NewProperty, OldProperty, dark_mode_visitor::declaration_block::ShouldRemoveImportant,
};

use super::colors::ShouldModifyTransparentColors;
use super::declaration_block::{DeclarationBlockVisitor, ShouldStoreOverriddenProps};

/// Walks through the style attribute, detects which
/// color needs to be adjusted to dark theme.
/// It modifies original stylesheet by removing `!important` flag if necessary.
/// The result of the dark-mode theming is available under [`StyleAttributeVisitor::overrides`] method.
///
#[derive(Default, Clone, Debug)]
pub(crate) struct StyleAttributeVisitor {
    overridden: Vec<OldProperty>,
    overrides: Vec<NewProperty>,

    should_modify_transparent_colors: ShouldModifyTransparentColors,

    pub printer_options: PrinterOptions<'static>,
}
impl StyleAttributeVisitor {
    pub fn new(should_modify_transparent_colors: ShouldModifyTransparentColors) -> Self {
        Self {
            should_modify_transparent_colors,
            ..Default::default()
        }
    }

    pub fn overrides(self) -> (Vec<OldProperty>, Vec<NewProperty>) {
        (self.overridden, self.overrides)
    }
}

impl Visitor<'_> for StyleAttributeVisitor {
    type Error = Infallible;

    fn visit_types(&self) -> lightningcss::visitor::VisitTypes {
        visit_types!(RULES | PROPERTIES)
    }

    fn visit_declaration_block(
        &mut self,
        decls: &mut lightningcss::declaration::DeclarationBlock<'_>,
    ) -> Result<(), Self::Error> {
        let mut visitor = DeclarationBlockVisitor::new(
            ShouldStoreOverriddenProps::Yes,
            ShouldRemoveImportant::Yes,
            self.should_modify_transparent_colors,
            self.printer_options,
        );

        decls.visit(&mut visitor)?;

        let (overridden_properties, properties_overrides) = visitor.overrides();
        if !properties_overrides.is_empty() {
            self.overrides.extend(properties_overrides);
            self.overridden.extend(overridden_properties);
        }

        Ok(())
    }
}
