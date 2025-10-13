use crate::Transformer;

#[test]
fn transform_to_proton_schemes_basic() {
    let html = r#"
    <html>
        <body>
            <img src="https://example.com/image.png" />
            <img src="http://test.com/photo.jpg" />
            <div style="background-image: url('https://background.com/bg.png');">Content</div>
        </body>
    </html>
    "#;
    let mut transformer = Transformer::new(html);
    let count = transformer.transform_to_proton_schemes();
    insta::assert_snapshot!(transformer.to_string());
    assert_eq!(count, 3);
}

#[test]
fn transform_from_proton_schemes_basic() {
    let html = r#"
    <html>
        <body>
            <img src="proton-https://example.com/image.png" />
            <img src="proton-http://test.com/photo.jpg" />
            <div style="background-image: url('proton-https://background.com/bg.png');">Content</div>
        </body>
    </html>
    "#;
    let mut transformer = Transformer::new(html);
    let count = transformer.transform_from_proton_schemes();
    insta::assert_snapshot!(transformer.to_string());
    assert_eq!(count, 3);
}

#[test]
fn transform_roundtrip() {
    let html = r#"
    <html>
        <body>
            <img src="https://example.com/image.png" />
            <img src="http://test.com/photo.jpg" />
            <div style="background-image: url('https://background.com/bg.png');">Content</div>
        </body>
    </html>
    "#;
    let mut transformer = Transformer::new(html);

    transformer.transform_to_proton_schemes();
    let after_to_proton = transformer.to_string();

    transformer.transform_from_proton_schemes();
    let after_roundtrip = transformer.to_string();

    // URLs should be back to original schemes
    assert!(after_to_proton.contains("proton-https://example.com/image.png"));
    assert!(after_to_proton.contains("proton-http://test.com/photo.jpg"));
    assert!(after_to_proton.contains("proton-https://background.com/bg.png"));

    assert!(after_roundtrip.contains("https://example.com/image.png"));
    assert!(after_roundtrip.contains("http://test.com/photo.jpg"));
    assert!(after_roundtrip.contains("https://background.com/bg.png"));

    // Should not contain any proton schemes after roundtrip
    assert!(!after_roundtrip.contains("proton-https://"));
    assert!(!after_roundtrip.contains("proton-http://"));
}

#[test]
fn transform_srcset_attribute() {
    let html = r#"
    <html>
        <body>
            <img srcset="https://example.com/small.jpg 480w, https://example.com/large.jpg 800w" />
        </body>
    </html>
    "#;
    let mut transformer = Transformer::new(html);
    let count = transformer.transform_to_proton_schemes();
    insta::assert_snapshot!(transformer.to_string());
    assert_eq!(count, 2);
}

#[test]
fn transform_relative_urls() {
    let html = r#"
    <html>
        <body>
            <img src="//example.com/image.png" />
            <img src="example.com/relative.jpg" />
            <div style="background-image: url('//cdn.example.com/bg.png');">Content</div>
        </body>
    </html>
    "#;
    let mut transformer = Transformer::new(html);
    let count = transformer.transform_to_proton_schemes();
    insta::assert_snapshot!(transformer.to_string());
    assert!(count > 0);
}

#[test]
fn transform_ignores_non_http_schemes() {
    let html = r#"
    <html>
        <body>
            <img src="data:image/png;base64,iVBORw0KGgo=" />
            <img src="cid:image123" />
            <a href="mailto:test@example.com">Email</a>
            <img src="https://example.com/image.png" />
        </body>
    </html>
    "#;
    let mut transformer = Transformer::new(html);
    let count = transformer.transform_to_proton_schemes();
    insta::assert_snapshot!(transformer.to_string());
    assert_eq!(count, 1);
}

#[test]
fn transform_mixed_content() {
    let html = r#"
    <html>
        <head>
            <style>
                .bg { background: url('https://example.com/style-bg.png'); }
                .another { background-image: url("http://test.com/another.jpg"); }
            </style>
        </head>
        <body>
            <img src="https://example.com/image.png" />
            <video poster="http://example.com/poster.jpg">
                <source src="https://example.com/video.mp4" />
            </video>
            <div data-src="https://lazy.com/lazy-image.png" style="background: url('https://inline.com/bg.png');">
                Content
            </div>
        </body>
    </html>
    "#;
    let mut transformer = Transformer::new(html);
    let count = transformer.transform_to_proton_schemes();
    insta::assert_snapshot!(transformer.to_string());
    assert!(count >= 6);
}
