//! This module provides features to strip UTM trackers from an URL as well as a transformer
//! pass which strips all UTM trackers from HTML links.
//!
//! UTM Codes (or UTM Tags) are a way to track traffic that is coming to your website from a
//! specific platform. UTM codes have two components: the tracking variable and the UTM parameters.
//! There are five parameters that you can use for tracking traffic. These parameters are source,
//! medium, campaign, term (keyword), and content.
//!

#[cfg(test)]
#[path = "tests/utm.rs"]
mod tests;

use kuchikiki::NodeRef;
use std::collections::{BTreeSet, HashSet};
use std::sync::LazyLock;
use url::Url;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Url: {0}")]
    Url(#[from] url::ParseError),
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone)]
pub struct StrippedUTM {
    pub original: Url,
    pub cleaned: Url,
}

#[must_use]
/// Strip UTM trackers from all HTML links in the given `document`.
pub fn strip(document: NodeRef) -> BTreeSet<StrippedUTM> {
    let Ok(select) = document.select("[href]") else {
        return BTreeSet::default();
    };

    let mut res = BTreeSet::new();
    for element in select {
        let mut attributes = element.attributes.borrow_mut();
        let Some(value) = attributes.get_mut("href") else {
            continue;
        };

        // The only possible error that can happen is if any of the nodes contains an
        // invalid (or a relative) url.
        // We don't throw the error back because the transformer doesn't care if the HTML
        // contains invalid links (how is it supposed to recover?)
        // We also don't error because that would short circuit and leave some tags unstripped.
        let Ok(url) = Url::parse(value) else {
            continue;
        };

        let original = url.clone();
        let Some(cleaned) = strip_from_url(url) else {
            continue;
        };
        *value = cleaned.to_string();
        res.insert(StrippedUTM { original, cleaned });
    }
    res
}

/// Removes UTM parameters from a given `url`.
/// Returns new URL if anything was stripped.
///
/// # Example
///
/// ```
/// use url::Url;
/// use proton_mail_html_transformer::utm::strip_from_url;
///
/// if let Ok(url) = Url::parse("https://example.com/?utm_source=example") {
///     let new_url  = strip_from_url(url);
///     assert_eq!(new_url.unwrap().as_str(), "https://example.com/");
/// }
/// ```
///
#[must_use]
pub fn strip_from_url(url: Url) -> Option<Url> {
    // TODO: [ET-84] We should not clone the URL, but we need to do it for now.
    let mut stripped_url = url.clone();
    stripped_url.set_query(None);

    let mut stripped_anything = false;
    for (key, value) in url.query_pairs() {
        if GLOBAL_RULES.contains(&key.to_lowercase().as_ref()) {
            stripped_anything = true;
            continue;
        }
        let mut query_pairs = stripped_url.query_pairs_mut();

        if value.is_empty() {
            query_pairs.append_key_only(&key);
        } else {
            query_pairs.append_pair(&key, &value);
        }
    }

    stripped_anything.then_some(stripped_url)
}

/// Removes UTM parameters from an `url` defined as a string.
pub fn strip_from_string(url: &str) -> Result<Option<Url>, url::ParseError> {
    let url = Url::parse(url)?;
    Ok(strip_from_url(url))
}

static GLOBAL_RULES: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    HashSet::from_iter([
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
    ])
});
