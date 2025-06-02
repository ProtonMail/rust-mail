use crate::{
    Transformer,
    transforms::{ColorMode, styles::BrowserCapabilities},
};

#[test]
fn inject_style_text_color_stylesheet_query_supported() {
    let html = include_str!("../../tests/htmls/styles/with_text_color_in_stylesheet.html");
    let mut html = Transformer::new(html);
    html.inject_dark_mode(
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
    );
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn inject_style_text_color_stylesheet_query_not_supported() {
    let html = include_str!("../../tests/htmls/styles/with_text_color_in_stylesheet.html");
    let mut html = Transformer::new(html);
    html.inject_dark_mode(
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: false,
        },
    );
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn inject_style_if_media_size_is_used() {
    let html = include_str!("../../tests/htmls/styles/with_media_size_in_stylesheet.html");
    let mut html = Transformer::new(html);
    html.inject_dark_mode(
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
    );
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn inject_style_check_contrast() {
    let html = include_str!("../../tests/htmls/styles/contrast.html");
    let mut html = Transformer::new(html);
    html.inject_dark_mode(
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
    );
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn inject_style_inline_attributes() {
    let html = include_str!("../../tests/htmls/styles/inline_attributes.html");
    let mut html = Transformer::new(html);
    html.inject_dark_mode(
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
    );
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn inject_style_transparency_handling() {
    let html = include_str!("../../tests/htmls/styles/transparent_colors.html");
    let mut html = Transformer::new(html);
    html.inject_dark_mode(
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
    );
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn inject_style_to_another_target() {
    let html = include_str!("../../tests/htmls/styles/with_text_color_in_stylesheet.html");
    let mut html = Transformer::new(html);
    let head = html.inject_dark_mode_to_another_target(
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
    );
    insta::assert_snapshot!(html.to_string());
    insta::assert_snapshot!(head);
}
