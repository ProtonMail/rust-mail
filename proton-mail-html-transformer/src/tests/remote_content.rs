#![allow(non_snake_case)]

use crate::Transformer;
use html5ever::tendril::TendrilSink;

// Note: If you need more test cases, it is recommended to set the transformed attribute
// at the end of the element since that is where it will be inserted after transformation.

const TEST_DOCUMENT: &str = r##"
<section>
    <svg id="svigi" width="5cm" height="4cm" version="1.1"
    xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
        <image x="0" y="0" height="50px" width="50px" xlink:href="firefox.jpg" />
        <image x="0" y="0" height="50px" width="50px" xlink:href="chrome.jpg" />
        <image x="0" y="0" height="50px" width="50px" href="svg-href.jpg" />
    </svg>
    <div>
        <img border="0" usemap="#fp" src="cats.jpg ">
        <map name="fp">
            <area coords="0,0,800,800" href="proton_exploit.html" shape="rect" target="_blank" >
        </map>
    </div>

    <img width="" height="" alt="" src="mon-image.jpg" srcset="mon-imageHD.jpg 2x">
    <img width="" height="" alt="" src="lol-image.jpg" srcset="lol-imageHD.jpg 2x">
    <img width="" height="" alt="" data-src="lol-image.jpg">
    <a href="lol-image.jpg">Alll</a>
    <a href="jeanne-image.jpg">Alll</a>
    <div background="jeanne-image.jpg">Alll</div>
    <div background="jeanne-image2.jpg">Alll</div>
    <p style="font-size:10.0pt;font-family:\\2018Calibri\\2019;color:black">
        Example style that caused regexps to crash
    </p>
    <img id="babase64" src="data:image/jpg;base64,iVBORw0KGgoAAAANSUhEUgAABoIAAAVSCAYAAAAisOk2AAAMS2lDQ1BJQ0MgUHJv
    ZmlsZQAASImVVwdYU8kWnltSSWiBUKSE3kQp0qWE0CIISBVshCSQUGJMCCJ2FlkF
    1y4ioK7oqoiLrgWQtaKudVHs/aGIysq6WLCh8iYF1tXvvfe9831z758z5/ynZO69
    MwDo1PKk0jxUF4B8SYEsITKUNTEtnUXqAgSgD1AwGozk8eVSdnx8DIAydP+nvLkO"
    />
</section>
"##;

const TEST_DOCUMENT_REMOTE_CONTENT_DISABLED: &str = r##"
<section>
    <svg id="svigi" width="5cm" height="4cm" version="1.1"
    xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
        <image x="0" y="0" height="50px" width="50px" xlink:proton-href="firefox.jpg" />
        <image x="0" y="0" height="50px" width="50px" xlink:proton-href="chrome.jpg" />
        <image x="0" y="0" height="50px" width="50px" proton-href="svg-href.jpg" />
    </svg>
    <div>
        <img border="0" usemap="#fp" proton-src="cats.jpg ">
        <map name="fp">
            <area coords="0,0,800,800" href="proton_exploit.html" shape="rect" target="_blank" >
        </map>
    </div>

    <img width="" height="" alt="" proton-src="mon-image.jpg" proton-srcset="mon-imageHD.jpg 2x">
    <img width="" height="" alt="" proton-src="lol-image.jpg" proton-srcset="lol-imageHD.jpg 2x">
    <img width="" height="" alt="" proton-data-src="lol-image.jpg">
    <a href="lol-image.jpg">Alll</a>
    <a href="jeanne-image.jpg">Alll</a>
    <div proton-background="jeanne-image.jpg">Alll</div>
    <div proton-background="jeanne-image2.jpg">Alll</div>
    <p style="font-size:10.0pt;font-family:\\2018Calibri\\2019;color:black">
        Example style that caused regexps to crash
    </p>
    <img id="babase64" proton-src="data:image/jpg;base64,iVBORw0KGgoAAAANSUhEUgAABoIAAAVSCAYAAAAisOk2AAAMS2lDQ1BJQ0MgUHJv
    ZmlsZQAASImVVwdYU8kWnltSSWiBUKSE3kQp0qWE0CIISBVshCSQUGJMCCJ2FlkF
    1y4ioK7oqoiLrgWQtaKudVHs/aGIysq6WLCh8iYF1tXvvfe9831z758z5/ynZO69
    MwDo1PKk0jxUF4B8SYEsITKUNTEtnUXqAgSgD1AwGozk8eVSdnx8DIAydP+nvLkO"
    />
</section>
"##;

#[test]
fn disable_remote_elements() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    transformer.disable_remote_content().unwrap();
    let output = transformer.to_string();

    let expected = kuchikiki::parse_html().one(TEST_DOCUMENT_REMOTE_CONTENT_DISABLED);
    assert_eq!(expected.to_string(), output.to_string());
}

#[test]
fn enable_remote_elements() {
    let mut transformer = Transformer::new(TEST_DOCUMENT_REMOTE_CONTENT_DISABLED);
    transformer.enable_remote_content().unwrap();
    let output = transformer.to_string();

    let expected = kuchikiki::parse_html().one(TEST_DOCUMENT);
    assert_eq!(expected.to_string(), output.to_string());
}

#[test]
fn disable_enable_remote_elements_cycle() {
    let mut transformer = Transformer::new(TEST_DOCUMENT);
    transformer.disable_remote_content().unwrap();
    transformer.enable_remote_content().unwrap();
    let output = transformer.to_string();

    let expected = kuchikiki::parse_html().one(TEST_DOCUMENT);
    assert_eq!(expected.to_string(), output.to_string());
}
