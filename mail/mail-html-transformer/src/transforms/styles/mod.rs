use std::collections::{BTreeSet, HashMap};
use std::fmt::Write;
use std::{cell::RefCell, collections::BTreeMap};

pub use capabilities::BrowserCapabilities;

use crate::css_parser::{parse_style_attribute, parse_stylesheet};
use dark_mode_visitor::{StyleAttributeVisitor, StylesheetVisitor};
use html5ever::{LocalName, QualName, namespace_url};
use itertools::Itertools;
use kuchikiki::{Attribute, Attributes, ElementData, NodeData, NodeDataRef, NodeRef};
use lightningcss::traits::{Parse, ToCss};
use lightningcss::values::color::{CssColor, HSL};
use lightningcss::{
    printer::PrinterOptions,
    properties::Property,
    stylesheet::{StyleAttribute, StyleSheet},
    values::color::RGBA,
    visitor::Visit,
};
use support_level::DarkStyleSupportLevel;

use crate::transforms::styles::colors::{HSLExt, hsla_for_dark_mode};

use super::ColorMode;

mod capabilities;
mod colors;
mod dark_mode_visitor;
mod support_level;

/// Reverts dark mode injection in inline attributes.
/// This function removes modified `style` attribute and restores original style from `data-proton-original-style` attribute.
pub fn revert_dark_mode_in_inline_attributes(document: &NodeRef) {
    let Ok(res) = document.select("[data-proton-original-style]") else {
        tracing::warn!("Could not select nodes with data-proton-original-style attribute");
        return;
    };

    for element in res {
        // SAFETY: we know that the attribute exists, because we selected it
        let style = element
            .attributes
            .borrow_mut()
            .remove("data-proton-original-style")
            .unwrap();
        element.attributes.borrow_mut().insert("style", style.value);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IncludeFullStaticCss {
    /// Should include full `./light.css`, `./dark.css` etc.
    Yes,
    /// Should include only a comment with a name of the file.
    /// Used in tests to avoid including full CSS files.
    No,
}

pub fn inject_common_css(document: &NodeRef) {
    inject_style(document, include_str!("./common.css"));
}

/// This function provides stylesheets for dark mode in plaintext messages.
/// In plaintext we do not need to parse HTML/CSS and just need to return static
/// stylesheets builtin in the SDK.
///
pub fn dark_mode_for_plaintext(
    mode: ColorMode,
    capabilities: BrowserCapabilities,
    include_full_static_css: IncludeFullStaticCss,
) -> &'static str {
    let level = DarkStyleSupportLevel::new_for_plaintext(mode, capabilities);

    let BrowserCapabilities {
        supports_dark_mode_via_media_query,
    } = capabilities;

    match (level, supports_dark_mode_via_media_query) {
        (DarkStyleSupportLevel::NoDarkMode, false) => {
            // If dark mode is currently not supported, let's just inject static css style.
            //
            if matches!(include_full_static_css, IncludeFullStaticCss::Yes) {
                concat!(include_str!("./colors.css"), include_str!("./light.css"))
            } else {
                "/* <light_css> */"
            }
        }
        (_, false) => {
            // We detected, that the message can be safely rendered in the dark mode.
            if matches!(include_full_static_css, IncludeFullStaticCss::Yes) {
                concat!(include_str!("./colors.css"), include_str!("./dark.css"))
            } else {
                "/* <dark_css> */"
            }
        }
        (_, true) => {
            // Browser supports `@media (prefers-color-scheme: dark)`.
            // So instead switching between light/dark CSS we can inject merged one
            if matches!(include_full_static_css, IncludeFullStaticCss::Yes) {
                concat!(
                    include_str!("./colors.css"),
                    include_str!("./light_and_dark.css")
                )
            } else {
                "/* <light_and_dark_css> */"
            }
        }
    }
}

/// Injects the data-proton-message attrubute to the html tag.
/// Used to create a selector with bigger specificity than any provided by the sender.
pub fn inject_root_selector_to_html(document: &NodeRef) {
    let Ok(html) = document.select_first("html") else {
        tracing::warn!("Could not select <html /> tag in the message body");
        return;
    };

    html.attributes
        .borrow_mut()
        .insert("data-protonmail-message", "true".to_owned());
}

pub struct InjectDarkModeOptions<'a> {
    /// The email address of the sender. Example: `test@pm.me`
    pub sender: Option<&'a str>,
    pub mode: ColorMode,
    pub capabilities: BrowserCapabilities,
    /// The CSS selector of the root of message.
    /// In case of viewing message, it is usually data attribute pointing to the `html` tag.
    /// In case of composer, it is ID pointing to custom editor that wraps the message.
    /// Used to create a selector with bigger specificity than any provided by the sender.
    pub root_selector: String,
    pub include_full_static_css: IncludeFullStaticCss,
    /// List of senders (email addresses, example: `test@pm.me`) that we trust that they support dark mode natively.
    pub trusted_senders: &'a [&'a str],
}

/// Adjusts style of the message to the light/dark mode.
/// In case of light mode only slight changes are applied.
/// In case of the dark mode, this function scans all styles provided by the sender,
/// checks whether the style is applicable in the dark mode and if not - modifies
/// the style of the message to suit better the theme.
///
/// Parameters:
/// * `source` - the source HTML document. Usually a message fetched from remote. Might be modified by removing `!important` flag from
///   styles and attributes.
/// * `target` - the target HTML document. Stylesheets and CSS supplements are appended to the head of the document.
///
/// # Difference between `source` and `target`
/// In the view mode of the message, both nodes are pointing to the same document.
/// However in the composer, `source` is the message being edited, while `target` is the head of HTML editor that wraps
/// the message. Styles appended to the `target` are not sent to the recipient.
pub fn inject_dark_mode(source: NodeRef, target: NodeRef, options: InjectDarkModeOptions) {
    let InjectDarkModeOptions {
        sender,
        mode,
        capabilities,
        root_selector,
        include_full_static_css,
        trusted_senders,
    } = options;

    let level = DarkStyleSupportLevel::new_for_html(sender, mode, trusted_senders, capabilities);

    let BrowserCapabilities {
        supports_dark_mode_via_media_query,
    } = capabilities;

    tracing::debug!("Dark style support level: {level:?}");
    tracing::debug!("Supports dark mode via media query: {supports_dark_mode_via_media_query}");

    match (level, supports_dark_mode_via_media_query) {
        (DarkStyleSupportLevel::NoDarkMode, false) => {
            // If dark mode is currently not supported, let's just inject static css style.
            //
            if matches!(include_full_static_css, IncludeFullStaticCss::Yes) {
                inject_style(
                    &target,
                    concat!(include_str!("./colors.css"), include_str!("./light.css")),
                );
            } else {
                inject_style(&target, "/* <light_css> */");
            }
        }
        (DarkStyleSupportLevel::Native, false) => {
            // We detected, that the message can be safely rendered in the dark mode.
            // We just need to inject our style.
            if matches!(include_full_static_css, IncludeFullStaticCss::Yes) {
                inject_style(
                    &target,
                    concat!(include_str!("./colors.css"), include_str!("./dark.css")),
                );
            } else {
                inject_style(&target, "/* <dark_css> */");
            }
        }
        (DarkStyleSupportLevel::NoDarkMode | DarkStyleSupportLevel::Native, true) => {
            // Browser supports `@media (prefers-color-scheme: dark)`. So instead switching between light/dark CSS we can inject merged one
            if matches!(include_full_static_css, IncludeFullStaticCss::Yes) {
                inject_style(
                    &target,
                    concat!(
                        include_str!("./colors.css"),
                        include_str!("./light_and_dark.css")
                    ),
                );
            } else {
                inject_style(&target, "/* <light_and_dark_css> */");
            }
        }
        (DarkStyleSupportLevel::Injected, supports_media_query) => {
            // In order to support dark mode, we need to analyze all colors used by the message.
            // If message sets anything to a color, we shall transform it to HSL color space,
            // then check if the contrast is sufficient comparing to our background color.
            //
            // 1. If yes, we can keep existing color.
            // 2. If not, we shall generate a CSS override (by removing `!important` from original place and adding new rule afterwards)
            //     that would use transformed color (keeping the same hue and saturation but changed light component).
            let maybe_supplement_css = sanitize_dark_mode(&source, root_selector);

            if supports_media_query {
                if matches!(include_full_static_css, IncludeFullStaticCss::Yes) {
                    inject_style(
                        &target,
                        concat!(
                            include_str!("./colors.css"),
                            include_str!("./light_and_dark.css")
                        ),
                    );
                } else {
                    inject_style(&target, "/* <light_and_dark_css> */");
                }

                if let Some(supplement_css) = maybe_supplement_css {
                    inject_style(
                        &target,
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
                if matches!(include_full_static_css, IncludeFullStaticCss::Yes) {
                    inject_style(
                        &target,
                        concat!(include_str!("./colors.css"), include_str!("./dark.css")),
                    );
                } else {
                    inject_style(&target, "/* <dark_css> */");
                }

                if let Some(supplement_css) = maybe_supplement_css {
                    inject_style(&target, &supplement_css);
                }
            }
        }
    }
}

fn sanitize_dark_mode(document: &NodeRef, root_selector: String) -> Option<String> {
    let maybe_supplement_for_stylesheets =
        sanitize_dark_mode_in_stylesheets(document, &root_selector);
    let maybe_supplement_for_inline_attributes =
        sanitize_dark_mode_in_inline_attributes(document, &root_selector);
    let maybe_supplement_for_deprecated_attributes =
        sanitize_dark_mode_in_deprecated_attributes(document, &root_selector);

    if maybe_supplement_for_stylesheets.is_none()
        && maybe_supplement_for_inline_attributes.is_none()
        && maybe_supplement_for_deprecated_attributes.is_none()
    {
        return None;
    }

    let supplement_for_stylesheets = maybe_supplement_for_stylesheets.unwrap_or_default();
    let supplement_for_inline_attributes =
        maybe_supplement_for_inline_attributes.unwrap_or_default();
    let supplement_for_deprecated_attributes =
        maybe_supplement_for_deprecated_attributes.unwrap_or_default();

    Some(format!(
        "{supplement_for_stylesheets}\n{supplement_for_inline_attributes}\n{supplement_for_deprecated_attributes}"
    ))
}

// Not using `RGBA::new` because it contains clamping which is not const-friendly.
/// Hex representation: #191927
pub const DARK_MODE_BACKGROUND_COLOR: RGBA = RGBA {
    red: 0x19,
    green: 0x19,
    blue: 0x27,
    alpha: 0xFF,
};

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

/// Old value of the property. Used to select nodes with inline styles.
type OldProperty = String;

type StylesheetOverrides = BTreeMap<Selectors, BTreeSet<NewProperty>>;
type InlineStyleOverrides = BTreeMap<InlineSelector, BTreeSet<NewProperty>>;

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
///
/// For each color it checks whether luminance provides good enough contrast in the dark mode.
/// If yes, it keeps the color intact.
/// If not, it removes `!important` flag and adds the rule to overrides map
/// Returns None if the supplement is empty
fn sanitize_dark_mode_in_stylesheets(document: &NodeRef, root_selector: &str) -> Option<String> {
    let mut overrides = BTreeMap::new();

    let Ok(styles) = document.select("style") else {
        tracing::warn!("Could not select <style /> tags in the message body");
        return None;
    };

    for style in styles {
        let mut text_content = style.text_contents();
        let stylesheet = match parse_stylesheet(&mut text_content) {
            Ok(stylesheet) => stylesheet,
            Err(err) => {
                tracing::warn!("Could not parse stylesheet content");
                tracing::warn!("Error: {err:?}");
                tracing::warn!("Skipping...");
                continue;
            }
        };

        sanitize_dark_mode_in_stylesheet(stylesheet, &mut overrides, root_selector.to_owned());
    }

    if overrides.is_empty() {
        return None;
    }
    let mut style = String::new();
    for (selectors, properties) in overrides {
        let mut style_for_rule = properties.into_iter().join(";\n");

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

/// Parses all instances of `style="..."` attribute used in any node.
///
/// For each color it checks whether luminance provides good enough contrast in the dark mode.
/// If yes, it keeps the color intact.
/// If not, it removes `!important` flag and adds the rule to overrides map
/// Returns None if the supplement is empty
fn sanitize_dark_mode_in_inline_attributes(
    document: &NodeRef,
    root_selector: &str,
) -> Option<String> {
    let Ok(styles) = all_with_attribute(document, "style") else {
        return None;
    };

    let mut overrides = BTreeMap::new();

    for (tag, mut style) in styles {
        let style_attribute = match parse_style_attribute(&mut style) {
            Ok(style_attribute) => style_attribute,
            Err(err) => {
                let tag = tag.name.local.to_string();
                tracing::warn!("Could not parse style attribute of tag `{tag}`");
                tracing::warn!("Error: {err:?}");
                tracing::warn!("Skipping...");
                continue;
            }
        };

        sanitize_dark_mode_in_inline_attribute(style_attribute, tag, &mut overrides);
    }

    if overrides.is_empty() {
        return None;
    }

    let mut style = String::new();
    for (tag_selector, properties) in overrides {
        let properties = properties.into_iter().join(";\n");

        write!(
            style,
            "{root_selector} {tag_selector} {{\n {properties}\n }}"
        )
        .expect("Written properties");
    }
    Some(style)
}

/// List of deprecated HTML attributes that are not CSS, but are still in use in some newsletters.
/// Those attributes contain a single color value
/// <https://www.w3.org/TR/2014/REC-html5-20141028/obsolete.html>
const DEPRECATED_ATTRIBUTES: &[&str] = &["bgcolor", "text", "color", "bordercolor"];

fn color_purpose_for_deprecated_attribute(attr: &str) -> ColorPurpose {
    match attr {
        "bgcolor" | "bordercolor" => ColorPurpose::Background,
        "text" | "color" | "alink" | "vlink" => ColorPurpose::Foreground,
        _ => unreachable!(),
    }
}

fn css_property_for_deprecated_attribute(attr: &str) -> &str {
    match attr {
        "bgcolor" => "background-color",
        "color" | "text" => "color",
        "bordercolor" => "border-color",
        _ => unreachable!(),
    }
}

/// Some email newsletters are using deprecated attributes like `bgcolor` or `text`.
///
/// For each color it checks whether luminance provides good enough contrast in the dark mode.
/// If yes, it keeps the color intact.
/// If not, it adds the rule to overrides map
/// Returns None if the supplement is empty
fn sanitize_dark_mode_in_deprecated_attributes(
    document: &NodeRef,
    root_selector: &str,
) -> Option<String> {
    let Ok(nodes) = all_with_any_attribute(document, DEPRECATED_ATTRIBUTES) else {
        return None;
    };

    let mut overrides: InlineStyleOverrides = BTreeMap::new();

    for node in nodes {
        let attributes = DEPRECATED_ATTRIBUTES
            .iter()
            .filter_map(|attr| {
                node.attributes
                    .borrow()
                    .get(*attr)
                    .map(|value| (*attr, value.to_string()))
            })
            .collect::<HashMap<_, _>>();

        for (attr, original_attr) in attributes {
            let color = match CssColor::parse_string(&original_attr) {
                Ok(color) => color,
                Err(err) => {
                    tracing::warn!("Could not parse color: {original_attr}. Error: {err:?}");
                    tracing::warn!("Skipping...");
                    continue;
                }
            };
            let Ok(color) = HSL::try_from(color) else {
                tracing::warn!(
                    "Could not convert color from deprecated attribute to HSL. Skipping..."
                );
                continue;
            };

            if color.is_transparent() {
                continue;
            }

            let purpose = color_purpose_for_deprecated_attribute(attr);
            let property = css_property_for_deprecated_attribute(attr);

            // It is a bit simplified approach - we are not calculating the proper contrast ratio here.
            let hsla = hsla_for_dark_mode(purpose, color);

            let new_color = CssColor::RGBA(hsla);
            let Ok(new_color) = new_color.to_css_string(PrinterOptions::default()) else {
                tracing::warn!("Could not convert color to CSS string. Skipping...");
                continue;
            };

            let mut node_selector = tag_selector(&node);
            write!(node_selector, r#"[{attr}="{original_attr}"]"#).expect("Write to string");

            overrides
                .entry(node_selector)
                .or_default()
                .insert(format!("{property}: {new_color};"));
        }
    }

    if overrides.is_empty() {
        return None;
    }

    let mut style = String::new();

    for (tag_selector, properties) in overrides {
        let properties = properties.into_iter().join(";\n");

        write!(
            style,
            "{root_selector} {tag_selector} {{\n {properties}\n }}"
        )
        .expect("Write to string");
    }

    Some(style)
}

fn sanitize_dark_mode_in_stylesheet(
    mut stylesheet: StyleSheet<'_, '_>,
    overrides: &mut StylesheetOverrides,
    root_selector: String,
) {
    let mut visitor = StylesheetVisitor::new(root_selector);
    _ = stylesheet.visit(&mut visitor); // Error is infallible anyway

    let visitor_overrides = visitor.overrides();
    if visitor_overrides.is_empty() {
        return;
    }

    // We do not modify original stylesheet

    for (key, value) in visitor_overrides {
        overrides.entry(key).or_default().extend(value);
    }
}

fn tag_selector(node: &NodeDataRef<ElementData>) -> String {
    let mut tag_selector = node.name.local.to_string();

    if let Some(id) = node.attributes.borrow().get("id") {
        write!(tag_selector, "[id=\"{id}\"]").expect("Write to string");
    }

    if let Some(klass) = node.attributes.borrow().get("class") {
        write!(tag_selector, "[class=\"{klass}\"]").expect("Write to string");
    }

    tag_selector
}

fn sanitize_dark_mode_in_inline_attribute(
    mut style_attribute: StyleAttribute<'_>,
    node: NodeDataRef<ElementData>,
    overrides: &mut InlineStyleOverrides,
) {
    let mut visitor = StyleAttributeVisitor::default();

    _ = style_attribute.visit(&mut visitor);

    let (overriden_properties, property_overrides) = visitor.overrides();
    if property_overrides.is_empty() {
        return;
    }

    let style = match style_attribute.to_css(PrinterOptions::default()) {
        Ok(style) => style,
        Err(err) => {
            tracing::error!("Could not write style attribute: {err:?}");
            return;
        }
    };

    let mut tag_selector = tag_selector(&node);

    // Joining is an equivalent of AND condition
    // a[style*="color: black"][style*="background-color: red"]
    // searches for tags <a /> tags that both have "color: black" AND "background-color: red".
    // It doesn't matter which style is first, nor if there is another property set in the CSS.
    //
    // [style *= "foo"] means "find every style that contains 'foo'".
    for prop in overriden_properties {
        write!(tag_selector, r#"[style*="{prop}"]"#).expect("Write to string");
    }

    overrides
        .entry(tag_selector)
        .or_default()
        .extend(property_overrides);

    let original_style = node
        .attributes
        .borrow_mut()
        .get_mut("style")
        .map(move |style_attr| std::mem::replace(style_attr, style.code));

    if let Some(original_style) = original_style {
        // In case it already exists, we do not want to override it.
        node.attributes
            .borrow_mut()
            .entry("data-proton-original-style")
            .or_insert(Attribute {
                prefix: None,
                value: original_style,
            });
    }
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
        .insert("type", "text/css".to_owned());

    let style_node = NodeRef::new(NodeData::Element(element_data));
    let text_node = NodeRef::new_text(style_text);

    style_node.append(text_node);

    element.as_node().append(style_node);
}

/// Tag name as from HTML `<div></div>` is the `div` combined with
/// selectors used to identified specific node
/// Usually:
/// * Classname `.foo`
/// * Id  `#foo`
/// * style attributes `[style*="foo: bar"]`
///
/// Joined together without delimiter
type InlineSelector = String;

/// Content of the style attribute. From `style="color: #fff"` is the `color: #fff`
type AttributeContent = String;

fn all_with_attribute(
    document: &NodeRef,
    attribute_name: &str,
) -> Result<impl Iterator<Item = (NodeDataRef<ElementData>, AttributeContent)>, ()> {
    let res = document
        .select(&format!("[{attribute_name}]"))
        .inspect_err(|()| {
            tracing::error!("Could not select nodes with {attribute_name} attribute");
        })?;

    Ok(res.map(move |element| {
        // SAFETY: unwrap is fine, the `.select()` ensures that the attribute exists
        let attribute = element
            .attributes
            .borrow()
            .get(attribute_name)
            .unwrap()
            .into();
        (element, attribute)
    }))
}

fn all_with_any_attribute(
    document: &NodeRef,
    attribute_names: &[&str],
) -> Result<impl Iterator<Item = NodeDataRef<ElementData>>, ()> {
    let selector = attribute_names
        .iter()
        .map(|attr| format!("[{attr}]"))
        .join(",");

    document.select(&selector).inspect_err(|()| {
        tracing::error!("Could not select nodes with any of the attributes");
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use html5ever::tendril::TendrilSink;
    use indoc::indoc;
    use pretty_assertions::assert_eq;
    use velcro::{btree_map, btree_set};

    #[test]
    fn visit_stylesheet() {
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

            html {
                color: #444;
            }
        "
        );

        let mut visitor = StylesheetVisitor::new("#protonmail-message".to_owned());
        let mut rule_string = rule.to_string();
        let mut stylesheet = parse_stylesheet(&mut rule_string).unwrap();
        stylesheet.visit(&mut visitor).unwrap();

        let expected = btree_map! {
            vec!["#protonmail-message .main".to_string()]: btree_set![
                "color: #fff !important".to_string()
            ],
            vec!["#protonmail-message .sub".to_string()]: btree_set![
                "color: #fff !important".to_string()
            ],
            vec!["html#protonmail-message".to_string()]: btree_set![
                "color: #fff !important".to_string()
            ],
        };

        assert_eq!(expected, visitor.overrides());

        let stylesheet = stylesheet.to_css(PrinterOptions::default()).unwrap().code;

        // Make sure we did not remove `!important` from stylesheet.
        assert_eq!(
            indoc!(
                ".main {
                  color: #000 !important;
                }

                .sub {
                  color: #444;
                }

                .another {
                  color: #aaa;
                }

                html {
                  color: #444;
                }
                "
            ),
            stylesheet
        );
    }

    #[test]
    fn injecting_style_does_not_escape_gt_lt() {
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

    #[test]
    fn fetching_all_style_attributes() {
        let html = r#"
            <html>
            <head>
            </head>
            <body style="color: red">
                <div>
                    <span>
                        <a href="http://wikipedia.com" style="background-color: yellow; color: black"> Wiki </a>
                    </span>
                </div>
            </body>
            </html>
        "#;

        let document = kuchikiki::parse_html().one(html);

        let result = all_with_attribute(&document, "style")
            .unwrap()
            .map(|(tag, style)| (tag.name.local.to_string(), style))
            .collect::<Vec<_>>();

        assert_eq!(
            vec![
                ("body".to_string(), "color: red".to_string()),
                (
                    "a".to_string(),
                    "background-color: yellow; color: black".to_string()
                )
            ],
            result
        );
    }

    #[test]
    fn fetching_all_deprecated_attributes() {
        let html = r#"
            <html>
            <head>
            </head>
            <body style="color: red">
                <div>
                    <span>
                        <a bgcolor="yellow"></a>
                        <span text="black"></span>
                        <marquee bgcolor="red" text="white"></marquee>
                    </span>
                </div>
            </body>
            </html>
        "#;

        let document = kuchikiki::parse_html().one(html);

        let result = all_with_any_attribute(&document, &["bgcolor", "text"])
            .unwrap()
            .map(|tag| tag.name.local.to_string())
            .collect::<Vec<_>>();

        assert_eq!(
            vec!["a".to_string(), "span".to_string(), "marquee".to_string(),],
            result
        );
    }

    #[test]
    fn visit_style_attribute() {
        let rule = "color: black !important; background-color: white";

        let printer_options = PrinterOptions::default();
        let mut visitor = StyleAttributeVisitor::default();
        let mut rule_string = rule.to_string();
        let mut attribute = parse_style_attribute(&mut rule_string).unwrap();
        attribute.visit(&mut visitor).unwrap();

        let expected = {
            (
                vec![
                    "color: #000".to_string(),
                    "background-color: #fff".to_string(),
                ],
                vec![
                    "background-color: #191927 !important".to_string(),
                    "color: #fff !important".to_string(),
                ],
            )
        };

        assert_eq!(expected, visitor.overrides());

        let attribute = attribute.to_css(printer_options).unwrap().code;

        // We not only generate override CSS but also remove `!important` from the original one
        assert_eq!("background-color: #fff; color: #000", attribute);
    }
}
