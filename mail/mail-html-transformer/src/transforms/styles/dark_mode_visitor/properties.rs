use std::convert::Infallible;

use lightningcss::{
    properties::{Property, PropertyId},
    vendor_prefix::VendorPrefix,
    visit_types,
    visitor::{Visit, Visitor},
};

use crate::transforms::styles::{ColorPurpose, PropertyWithPurpose};

use super::colors::{ColorVisitor, ShouldModifyTransparentColors};

/// All properties that might contain background color. Including all shorthands
const BACKGROUND_COLOR_RELATED_PROPERTIES: &[PropertyId] = &[
    PropertyId::Background, // Shorthand
    PropertyId::BackgroundColor,
    PropertyId::TextShadow,
    PropertyId::BoxShadow(VendorPrefix::all()),
    PropertyId::Border, // Shorthand
    PropertyId::BorderColor,
    PropertyId::BorderTop, // Shorthand
    PropertyId::BorderTopColor,
    PropertyId::BorderLeft, // Shorthand
    PropertyId::BorderLeftColor,
    PropertyId::BorderRight, // Shorthand
    PropertyId::BorderRightColor,
    PropertyId::BorderBottom, // Shorthand
    PropertyId::BorderBottomColor,
    PropertyId::BorderBlockStart, // Shorthand
    PropertyId::BorderBlockStartColor,
    PropertyId::BorderBlockEnd, // Shorthand
    PropertyId::BorderBlockEndColor,
    PropertyId::BorderInlineStart, // Shorthand
    PropertyId::BorderInlineStartColor,
    PropertyId::BorderInlineEnd, // Shorthand
    PropertyId::BorderInlineEndColor,
];

/// All properties that might contain color. Including all shorthands
const FOREGROUND_COLOR_RELATED_PROPERTIES: &[PropertyId] = &[
    PropertyId::Color,
    PropertyId::Outline, // Shorthand
    PropertyId::OutlineColor,
    PropertyId::TextDecoration(VendorPrefix::all()), // Shorthand
    PropertyId::TextDecorationColor(VendorPrefix::all()),
    PropertyId::TextEmphasis(VendorPrefix::all()), // Shorthand
    PropertyId::TextEmphasisColor(VendorPrefix::all()),
    PropertyId::WebKitTextFillColor(VendorPrefix::WebKit),
];

/// Goes through the list of properties, checks if the color is matching color scheme.
/// If no, then prepares the override
pub(crate) struct PropertiesVisitor<'i> {
    /// We keep cloned properties that we modified, so that we can later remove !important keyword
    pub(crate) modified: Vec<Property<'i>>,

    /// We keep the result of the visitor which is a list of properties with already adjusted colors.
    pub(crate) overrides: Vec<PropertyWithPurpose<'i>>,
    should_modify_transparent_colors: ShouldModifyTransparentColors,
}

impl<'i> PropertiesVisitor<'i> {
    pub fn new(should_modify_transparent_colors: ShouldModifyTransparentColors) -> Self {
        Self {
            modified: Vec::default(),
            overrides: Vec::default(),
            should_modify_transparent_colors,
        }
    }

    pub fn is_background(prop: &Property<'i>) -> bool {
        BACKGROUND_COLOR_RELATED_PROPERTIES.contains(&prop.property_id())
    }

    pub fn is_foreground(prop: &Property<'i>) -> bool {
        FOREGROUND_COLOR_RELATED_PROPERTIES.contains(&prop.property_id())
    }
}

impl<'i> Visitor<'i> for PropertiesVisitor<'i> {
    type Error = Infallible;

    fn visit_types(&self) -> lightningcss::visitor::VisitTypes {
        visit_types!(PROPERTIES)
    }

    fn visit_property(&mut self, property: &mut Property<'i>) -> Result<(), Self::Error> {
        let color_purpose = if Self::is_background(property) {
            ColorPurpose::Background
        } else if Self::is_foreground(property) {
            ColorPurpose::Foreground
        } else {
            // We can safely ignore this property.
            return Ok(());
        };

        // We clone the property so that we can mutate it in place.
        // By mutating we mean adjust the color to dark-mode-compliant.
        let mut new = property.clone();

        new.visit_children(&mut ColorVisitor::new(
            color_purpose,
            self.should_modify_transparent_colors,
        ))?;

        if &new == property {
            // We have not changed a single color. We do not need to generate override
            // nor remove the `!important` flag.
            return Ok(());
        }

        self.modified.push(property.clone());
        self.overrides.push(PropertyWithPurpose {
            property: new,
            color_purpose,
        });

        Ok(())
    }
}
