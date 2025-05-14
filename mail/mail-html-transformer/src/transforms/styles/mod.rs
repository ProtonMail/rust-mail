use std::{cell::RefCell, collections::BTreeMap};

pub use capabilities::BrowserCapabilities;

use dark_mode_visitor::DarkModeVisitor;
use html5ever::{LocalName, QualName, namespace_url};
use kuchikiki::{Attributes, ElementData, NodeData, NodeDataRef, NodeRef};
use lightningcss::{
    printer::PrinterOptions,
    properties::Property,
    stylesheet::{ParserOptions, StyleSheet},
    values::color::RGBA,
    visitor::Visit,
};
use support_level::DarkStyleSupportLevel;

use super::ColorMode;

mod capabilities;
mod dark_mode_visitor;
mod support_level;

/// Adjusts style of the message to the light/dark mode.
/// In case of light mode only slight changes are applied.
/// In case of the dark mode, this function scans all styles provided by the sender,
/// checks whether the style is applicable in the dark mode and if not - modifies
/// the style of the message to suit better the theme.
///
pub fn transform_style(document: NodeRef, mode: ColorMode, capabilities: BrowserCapabilities) {
    let level = DarkStyleSupportLevel::new(mode, &document, capabilities);

    let BrowserCapabilities {
        supports_dark_mode_via_media_query,
    } = capabilities;

    match (level, supports_dark_mode_via_media_query) {
        (DarkStyleSupportLevel::NoDarkMode, false) => {
            // If dark mode is currently not supported, let's just inject static css style.
            //
            inject_style(&document, include_str!("./light.css"));
        }
        (DarkStyleSupportLevel::Native, false) => {
            // We detected, that the message can be safely rendered in the dark mode.
            // We just need to inject our style.
            inject_style(&document, include_str!("./dark.css"));
        }
        (DarkStyleSupportLevel::NoDarkMode | DarkStyleSupportLevel::Native, true) => {
            // Browser supports `@media (prefers-color-scheme: dark)`. So instead switching between light/dark CSS we can inject merged one
            inject_style(&document, include_str!("./light_and_dark.css"));
        }
        (DarkStyleSupportLevel::Injected, supports_media_query) => {
            // In order to support dark mode, we need to analyze all colors used by the message.
            // If message sets anything to a color, we shall transform it to HSL color space,
            // then check if the contrast is sufficient comparing to our background color.
            //
            // 1. If yes, we can keep existing color.
            // 2. If not, we shall generate a CSS override (by removing `!important` from original place and adding new rule afterwards)
            //     that would use transformed color (keeping the same hue and saturation but changed light component).
            let maybe_supplement_css = sanitize_dark_mode_in_stylesheets(&document);
            // TODO: Inline attributes!

            if supports_media_query {
                inject_style(&document, include_str!("./light_and_dark.css"));

                if let Some(supplement_css) = maybe_supplement_css {
                    inject_style(
                        &document,
                        &format!(
                            r"
                  @media ( prefers-color-scheme: dark ) {{
                      {supplement_css}
                  }}
                "
                        ),
                    );
                }
            } else {
                inject_style(&document, include_str!("./dark.css"));
                if let Some(supplement_css) = maybe_supplement_css {
                    inject_style(&document, &supplement_css);
                }
            }
        }
    }
}

// TODO: replace with proper constant after `RGBA` gets const constructor.
//
/// Returns our constant color for background color.
/// Hex representation: #1C1B24
pub fn dark_mode_background_color() -> RGBA {
    RGBA::new(28, 27, 36, 1.0)
}

type Selector = String;

/// Represents `.class {}`, `@media () {}` blocks etc.
/// Example: List of `['@media (max-width: 1250px)', '.foo']` represents a CSS structure of:
/// ```ignore
/// @media (max-width: 1250px) {
///     .foo {
///     }
/// }
/// ```
type Selectors = Vec<Selector>;

/// Property with new value. It not only contains the colors because of shorthands.
/// For example if the shorthand defined:
/// ```ignore
/// border: 1px solid white;
/// ```
/// then we have to modify color component and later write:
/// ```ignore
/// border: 1px solid black;
///                   ^^^^^ - changed part
/// ```
/// So in the `NewProperty` we keep `border: 1px solid black`;
type NewProperty = String;

type StyleOverrides = BTreeMap<Selectors, Vec<NewProperty>>;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ColorPurpose {
    Foreground,
    Background,
}

/// Property with information if its for foreground or background
#[derive(Clone, Debug)]
struct PropertyWithPurpose<'i> {
    pub(crate) property: Property<'i>,
    pub(crate) color_purpose: ColorPurpose,
}

/// Parses all stylesheets embedded in `<style />` tags.
/// For each color it checks whether luminance provides good enough contrast in the dark mode.
/// If yes, it keeps the color intact.
/// If not, it removes `!important` flag and adds the rule to overrides map
/// Returns None if t he supplement is empty
fn sanitize_dark_mode_in_stylesheets(document: &NodeRef) -> Option<String> {
    let mut overrides = BTreeMap::new();

    let Ok(styles) = document.select("style") else {
        tracing::warn!("Could not select <style /> tags in the message body");
        return None;
    };

    for style in styles {
        let text_content = style.text_contents();
        let Ok(stylesheet) = StyleSheet::parse(&text_content, ParserOptions::default()) else {
            tracing::warn!("Could not parse stylesheet content. Skipping...");
            continue;
        };

        sanitize_dark_mode_in_stylesheet(stylesheet, style, &mut overrides, printer_options);
    }

    if overrides.is_empty() {
        return None;
    }
    let mut style = String::new();
    for (selectors, properties) in overrides {
        let mut style_for_rule = properties.join(";\n");

        // In reverse.
        // If we got ["@media(...)", ".foo"] then we basically want to wrap our properties first in
        // ".foo { properties }"
        // and then with @media
        // "@media(...) { .foo { properties }}"
        for selector in selectors.into_iter().rev() {
            style_for_rule = format!("{selector} {{\n{style_for_rule}\n }}");
        }
        style += &style_for_rule;
    }
    Some(style)
}

// Because PrinterOptions are not clonable
// TODO: Make PR to lightningCSS
fn printer_options() -> PrinterOptions<'static> {
    PrinterOptions {
        minify: true,
        ..Default::default()
    }
}

fn sanitize_dark_mode_in_stylesheet(
    mut stylesheet: StyleSheet<'_, '_>,
    node: NodeDataRef<ElementData>,
    overrides: &mut StyleOverrides,
    printer_options: fn() -> PrinterOptions<'static>,
) {
    let mut visitor = DarkModeVisitor::new(printer_options);
    _ = stylesheet.visit(&mut visitor); // Error is infallible anyway

    let visitor_overrides = visitor.overrides();
    if visitor_overrides.is_empty() {
        return;
    }

    // If we found anything to change, we want to re-write the style.
    let css = match stylesheet.to_css(printer_options()) {
        Ok(css) => css,
        Err(err) => {
            tracing::error!("Could not write CSS: {err:?}");
            return;
        }
    };

    for (key, value) in visitor_overrides {
        overrides.entry(key).or_default().extend(value);
    }

    let text_node = NodeRef::new(NodeData::Text(RefCell::new(css.code)));

    // Clear existing text
    let existing_children = node.as_node().children().collect::<Vec<_>>();
    for child in existing_children {
        child.detach();
    }

    // Then append new text
    node.as_node().append(text_node);
}

fn inject_style(document: &NodeRef, style_text: &str) {
    let element = document.select_first("head").unwrap(); // kuckikiki always adds it

    let qual_name = QualName::new(None, html5ever::ns!(html), LocalName::from("style"));

    #[allow(clippy::default_trait_access)]
    let element_data = ElementData {
        name: qual_name,
        attributes: RefCell::new(Attributes {
            map: Default::default(),
        }),
        template_contents: None,
    };

    element_data
        .attributes
        .borrow_mut()
        .insert("style", "text/css".to_owned());

    let style_node = NodeRef::new(NodeData::Element(element_data));
    let text_node = NodeRef::new_text(style_text);

    style_node.append(text_node);

    element.as_node().append(style_node);
}

#[cfg(test)]
mod tests {
    use super::*;
    use indoc::indoc;
    use pretty_assertions::assert_eq;

    #[test]
    fn visit_rule() {
        let rule = indoc!(
            r"
            .main {
                color: black !important;
            }

            .sub {
                color: #444;
            }

            .another {
                color: #aaa;
            }
        "
        );

        let printer_options = || PrinterOptions::default();
        let mut visitor = DarkModeVisitor::new(printer_options);
        let mut stylesheet = StyleSheet::parse(rule, ParserOptions::default()).unwrap();
        stylesheet.visit(&mut visitor).unwrap();

        let expected = velcro::btree_map! {
            vec![".main".to_string()]: vec![
                "color: #fff !important".to_string()
            ],
            vec![".sub".to_string()]: vec![
                "color: #fff !important".to_string()
            ],
        };

        assert_eq!(expected, visitor.overrides());

        let stylesheet = stylesheet.to_css(printer_options()).unwrap().code;

        // We not only generate override CSS but also remove `!important` from the original one
        assert_eq!(
            indoc!(
                ".main {
                  color: #000;
                }

                .sub {
                  color: #444;
                }

                .another {
                  color: #aaa;
                }
                "
            ),
            stylesheet
        );
    }

    #[test]
    fn injecting_style_does_not_escape_gt_lt() {
        use html5ever::tendril::TendrilSink;

        // https://www.w3.org/TR/mediaqueries-4/ EXAMPLE 26 - < sign is a valid syntax here
        let style = "
            @media (width < 800px) {
                
            }
        ";

        let empty = "
            <html>
            <head>
            </head>
            </html>
        ";

        let document = kuchikiki::parse_html().one(empty);

        inject_style(&document, style);

        let html = document.to_string();

        insta::assert_snapshot!(html);
    }
}
