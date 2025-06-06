use crate::{
    Transformer,
    transforms::{ColorMode, styles::BrowserCapabilities},
};

#[test]
fn inject_style_text_color_stylesheet_query_supported() {
    let html = include_str!("../../tests/htmls/styles/with_text_color_in_stylesheet.html");
    let mut html = Transformer::new(html);
    html.inject_dark_mode(
        "",
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
        "",
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
        "",
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
        "",
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
        "",
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
    );
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn inject_style_deprecated_attributes() {
    let html = include_str!("../../tests/htmls/styles/deprecated_attributes.html");
    let mut html = Transformer::new(html);
    html.inject_dark_mode(
        "",
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
    );
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn revert_dark_mode_in_inline_attributes() {
    let original_html = include_str!("../../tests/htmls/styles/inline_attributes.html");
    // First, inject dark mode - just a copy of previous test
    let mut html = Transformer::new(original_html);
    // But we are not interested in HEAD, just the changes that could end up in sent message.
    html.inject_dark_mode_to_another_target(
        None,
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
        "#protonmail-message".to_owned(),
    );

    let html = html.to_string();

    // Now, revert dark mode
    let mut html = Transformer::new(&html);
    html.revert_dark_mode_in_inline_attributes();
    let html_after_revert = html.to_string();

    // We are not using `assert_eq!` here because HTML formatting might be different while the content is identical.
    insta::assert_snapshot!(html_after_revert);
}

#[test]
fn inject_style_transparency_handling() {
    let html = include_str!("../../tests/htmls/styles/transparent_colors.html");
    let mut html = Transformer::new(html);
    html.inject_dark_mode(
        "",
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
        None,
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
        "#protonmail-message".to_owned(),
    );
    insta::assert_snapshot!(html.to_string());
    insta::assert_snapshot!(head);
}

#[test]
fn inject_style_to_another_target_twice() {
    // This is to see what will happen if we execute draft.composer_head() twice in a row.

    let capabilities = BrowserCapabilities {
        supports_dark_mode_via_media_query: true,
    };
    let html = include_str!("../../tests/htmls/styles/with_text_color_in_stylesheet.html");
    let mut html = Transformer::new(html);
    let head_after_first_pass = html.inject_dark_mode_to_another_target(
        None,
        ColorMode::DarkMode,
        capabilities,
        "#protonmail-message".to_owned(),
    );
    let html_after_first_pass = html.to_string();

    // Second pass
    let mut html = Transformer::new(&html_after_first_pass);
    let head_after_second_pass = html.inject_dark_mode_to_another_target(
        None,
        ColorMode::DarkMode,
        capabilities,
        "#protonmail-message".to_owned(),
    );
    let html_after_second_pass = html.to_string();

    // It should not affect it anymore
    assert_eq!(head_after_first_pass, head_after_second_pass);
    assert_eq!(html_after_first_pass, html_after_second_pass);
}

#[test]
fn doesnt_inject_style_for_message_that_handles_dark_mode_natively() {
    let html = include_str!("../../tests/htmls/styles/native_dark_mode_support.html");

    let mut html = Transformer::new(html);
    html.inject_dark_mode(
        "",
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
    );
    insta::assert_snapshot!(html.to_string());
}

#[test]
fn inject_style_for_message_that_handles_dark_mode_natively_but_sender_is_untrusted() {
    let html = include_str!("../../tests/htmls/styles/native_dark_mode_support.html");

    let mut html = Transformer::new(html);
    html.inject_dark_mode(
        "test@pm.me",
        ColorMode::DarkMode,
        BrowserCapabilities {
            supports_dark_mode_via_media_query: true,
        },
    );
    insta::assert_snapshot!(html.to_string());
}

mod regressions {
    use crate::{
        Transformer,
        transforms::{ColorMode, styles::BrowserCapabilities},
    };

    // Bugs caught live
    #[test]
    fn table_bgcolor() {
        let html = include_str!("../../tests/htmls/styles/regressions/table-bgcolor.html");
        let mut html = Transformer::new(html);
        html.inject_dark_mode(
            "",
            ColorMode::DarkMode,
            BrowserCapabilities {
                supports_dark_mode_via_media_query: true,
            },
        );
        insta::assert_snapshot!(html.to_string());
    }
}
