use crate::{
    Transformer,
    transforms::{ColorMode, styles::BrowserCapabilities},
};

#[test]
fn inject_style_text_color_stylesheet_query_supported() {
    let html = include_str!("../../tests/htmls/styles/with_text_color_in_stylesheet.html");
    let mut html = Transformer::new(html);
    html.inject_style(
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
    html.inject_style(
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
    html.inject_style(
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
    html.inject_style(
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
    );
    insta::assert_snapshot!(html.to_string());
}
