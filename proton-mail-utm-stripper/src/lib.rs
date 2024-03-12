//! Proton Mail UTM Stripper
//! A simple library to remove UTM parameters from URLs.

use lazy_static::lazy_static;
use std::collections::HashSet;
use url::Url;

/// Removes UTM parameters from a URL Object
///
/// # Example
///
/// ```
/// use url::Url;
/// use proton_mail_utm_stripper::remove_utm_parameters_from_url;
///
/// if let Ok(url) = Url::parse("https://example.com/?utm_source=example") {
///     let new_url = remove_utm_parameters_from_url(&url);
///     assert_eq!(new_url.as_str(), "https://example.com/");
/// }
/// ```
///
/// # Arguments
///
/// * `entry_url` - The Url to remove UTM parameters from.
pub fn remove_utm_parameters_from_url(entry_url: &Url) -> Url {
    // TODO: [ET-84] We should not clone the URL, but we need to do it for now.
    let mut url = entry_url.clone();
    url.set_query(None);

    for (key, value) in entry_url.query_pairs() {
        if !GLOBAL_RULES.contains(&key.as_ref()) {
            url.query_pairs_mut().append_pair(&key, &value);
        }
    }

    url
}

/// Cleans the URL by removing trailing and leading whitespaces and converting it to lowercase.
fn clean_entry_url(entry_url: &str) -> String {
    entry_url.trim().to_lowercase()
}

/// Removes UTM parameters from a string.
pub fn remove_utm_parameters_from_string(entry_url: &str) -> Result<Url, url::ParseError> {
    let cleaned_entry = clean_entry_url(entry_url);
    let url = Url::parse(&cleaned_entry)?;
    Ok(remove_utm_parameters_from_url(&url))
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

mod tests {
    #[test]
    fn test_remove_utm_parameters() {
        use crate::remove_utm_parameters_from_string;

        let url = "https://example.com/?UTM_SOURCE=example&utm_medium=example&utm_campaign=example";
        let new_url = remove_utm_parameters_from_string(url).unwrap();
        assert_eq!(new_url.as_str(), "https://example.com/");

        let url = "panda"; // Invalid URL
        let new_url = remove_utm_parameters_from_string(url);
        assert_eq!(new_url.is_err(), true);
    }
}
