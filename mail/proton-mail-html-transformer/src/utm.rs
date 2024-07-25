//! This module provides features to strip UTM trackers from an URL as well as a transformer
//! pass which strips all UTM trackers from HTML links.
//!
//! UTM Codes (or UTM Tags) are a way to track traffic that is coming to your website from a
//! specific platform. UTM codes have two components: the tracking variable and the UTM parameters.
//! There are five parameters that you can use for tracking traffic. These parameters are source,
//! medium, campaign, term (keyword), and content.
//!
use kuchikiki::NodeRef;
use lazy_static::lazy_static;
use std::collections::HashSet;
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid Selector")]
    Selector,
    #[error("Url: {0}")]
    Url(#[from] url::ParseError),
}

#[allow(clippy::missing_panics_doc)] // The select is well formed.
/// Strip UTM trackers from all HTML links in the given `document`.
///
/// # Errors
///
/// Returns error if an HTML href attribute is not a valid url.
pub fn strip(document: NodeRef) -> Result<(), Error> {
    let select = document.select("[href]").unwrap();

    for element in select {
        let mut attributes = element.attributes.borrow_mut();
        let Some(value) = attributes.get_mut("href") else {
            continue;
        };

        let new_value = strip_from_string(value)?;
        *value = new_value.to_string();
    }

    Ok(())
}

/// Removes UTM parameters from a given `url`.
///
/// # Example
///
/// ```
/// use url::Url;
/// use proton_mail_html_transformer::utm::strip_from_url;
///
/// if let Ok(url) = Url::parse("https://example.com/?utm_source=example") {
///     let new_url = strip_from_url(&url);
///     assert_eq!(new_url.as_str(), "https://example.com/");
/// }
/// ```
///
#[must_use]
pub fn strip_from_url(url: &Url) -> Url {
    // TODO: [ET-84] We should not clone the URL, but we need to do it for now.
    let mut stripped_url = url.clone();
    stripped_url.set_query(None);

    for (key, value) in url.query_pairs() {
        if !GLOBAL_RULES.contains(&key.to_lowercase().as_ref()) {
            stripped_url.query_pairs_mut().append_pair(&key, &value);
        }
    }

    stripped_url
}

/// Removes UTM parameters from an `url` defined as a string.
///
/// # Errors
///
/// Will return error if `url` cannot be parsed into an [`Url`]
pub fn strip_from_string(url: &str) -> Result<Url, url::ParseError> {
    let url = Url::parse(url)?;
    Ok(strip_from_url(&url))
}

lazy_static! {
    static ref GLOBAL_RULES: HashSet<&'static str> = HashSet::from_iter([
        // https://en.wikipedia.org/wiki/UTM_parameters
        "utm_source",
        "utm_medium",
        "utm_term",
        "utm_campaign",
        "utm_content",
        "utm_name",
        "utm_cid",
        "utm_reader",
        "utm_viz_id",
        "utm_pubreferrer",
        "utm_swu",
        "utm_social-type",
        "utm_brand",
        "utm_team",
        "utm_feeditemid",
        "utm_id",
        "utm_marketing_tactic",
        "utm_creative_format",
        "utm_campaign_id",
        "utm_source_platform",
        "utm_timestamp",
        "utm_souce",
        // ITM parameters, a variant of UTM parameters
        "itm_source",
        "itm_medium",
        "itm_term",
        "itm_campaign",
        "itm_content",
        "itm_channel",
        "itm_source_s",
        "itm_medium_s",
        "itm_campaign_s",
        "itm_audience",
        // INT parameters, another variant of UTM
        "int_source",
        "int_cmp_name",
        "int_cmp_id",
        "int_cmp_creative",
        "int_medium",
        "int_campaign",
        // piwik (https://github.com/DrKain/tidy-url/issues/49)
        "pk_campaign",
        "pk_cpn",
        "pk_source",
        "pk_medium",
        "pk_keyword",
        "pk_kwd",
        "pk_content",
        "pk_cid",
        "piwik_campaign",
        "piwik_cpn",
        "piwik_source",
        "piwik_medium",
        "piwik_keyword",
        "piwik_kwd",
        "piwik_content",
        "piwik_cid",
        // Google Ads
        "gclid",
        "ga_source",
        "ga_medium",
        "ga_term",
        "ga_content",
        "ga_campaign",
        "ga_place",
        "gclid",
        "gclsrc",
        // https://github.com/DrKain/tidy-url/issues/21
        "hsa_cam",
        "hsa_grp",
        "hsa_mt",
        "hsa_src",
        "hsa_ad",
        "hsa_acc",
        "hsa_net",
        "hsa_kw",
        "hsa_tgt",
        "hsa_ver",
        "hsa_la",
        "hsa_ol",
        // Facebook
        "fbclid",
        // Olytics
        "oly_enc_id",
        "oly_anon_id",
        // Vero
        "vero_id",
        "vero_conv",
        // Drip
        "__s",
        // HubSpot
        "_hsenc",
        "_hsmi",
        "__hssc",
        "__hstc",
        "__hsfp",
        "hsCtaTracking",
        // Marketo
        "mkt_tok",
        // Matomo (https://github.com/DrKain/tidy-url/issues/47)
        "mtm_campaign",
        "mtm_keyword",
        "mtm_kwd",
        "mtm_source",
        "mtm_medium",
        "mtm_content",
        "mtm_cid",
        "mtm_group",
        "mtm_placement",
        // Oracle Eloqua
        "elqTrackId",
        "elq",
        "elqaid",
        "elqat",
        "elqCampaignId",
        "elqTrack",
        // MailChimp
        "mc_cid",
        "mc_eid",
        // Other
        "ncid",
        "cmpid",
        "mbid",
        // Reddit Ads (https://github.com/DrKain/tidy-url/issues/31)
        "rdt_cid",
    ]);
}

#[test]
fn remove_from_url() {
    let url = "https://example.com/?UTM_SOURCE=example&utm_medium=example&utm_campaign=example&UserID=123";
    let new_url = strip_from_string(url).unwrap();
    assert_eq!(new_url.as_str(), "https://example.com/?UserID=123");

    let url = "panda"; // Invalid URL
    let new_url = strip_from_string(url);
    assert!(new_url.is_err());
}

#[test]
fn remove_with_transformer() {
    use crate::Transformer;
    use kuchikiki::traits::*;
    let input = r#"
<html>
    <body>
        <a href="https://ads.com?utm_source=tracker">bar</a>
    </body>
</html>
"#;

    let expected = r#"
<html>
    <body>
        <a href="https://ads.com/">bar</a>
    </body>
</html>
"#;

    // Parse and print so the results have the same formatting.
    let expected = kuchikiki::parse_html().one(expected).to_string();

    let mut transformer = Transformer::new(input);
    transformer.strip_utm().unwrap();
    let output = transformer.to_string();
    assert_eq!(expected, output);
}
