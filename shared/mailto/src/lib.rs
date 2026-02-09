use email_address::{EmailAddress, Options};
use std::str::FromStr;
use thiserror::Error;
use url::{ParseError, Url};

/// A `mailto:` link, as described in RFC 6068.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mailto {
    pub to: Vec<String>,
    pub cc: Vec<String>,
    pub bcc: Vec<String>,
    pub subject: Option<String>,
    pub body: Option<String>,
}

fn parse_email_address(email: &str) -> Result<EmailAddress, email_address::Error> {
    EmailAddress::parse_with_options(
        email,
        Options::default()
            .without_domain_literal()
            .with_display_text()
            .with_long_local_parts()
            .with_required_tld(),
    )
}

fn parse_emails(input: &str) -> Vec<String> {
    input
        .split([',', ';'])
        .map(|email| {
            let email = email.trim();

            let email = match percent_encoding::percent_decode(email.as_bytes()).decode_utf8() {
                Ok(email) => email.trim().to_string(),
                Err(_) => email.to_string(),
            };

            let Ok(email) = parse_email_address(&email) else {
                return email;
            };
            email.email()
        })
        .collect()
}

impl FromStr for Mailto {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url = Url::from_str(s).map_err(Error::InvalidUrl)?;

        if !url.scheme().eq_ignore_ascii_case("mailto") {
            return Err(Error::InvalidScheme(url.scheme().into()));
        }

        let mut this = Self::default();

        // ---

        let to = url.path().trim();

        if !to.is_empty() && to != "@" {
            this.to = parse_emails(to);
        }

        // ---

        for (k, v) in url.query_pairs() {
            let k = k.trim();
            let v = v.trim();

            match k.trim() {
                k if k.eq_ignore_ascii_case("to") => {
                    this.to.extend(parse_emails(v));
                }
                k if k.eq_ignore_ascii_case("cc") => {
                    this.cc.extend(parse_emails(v));
                }
                k if k.eq_ignore_ascii_case("bcc") => {
                    this.bcc.extend(parse_emails(v));
                }

                k if k.eq_ignore_ascii_case("subject") => {
                    this.subject = Some(v.into());
                }
                k if k.eq_ignore_ascii_case("body") => {
                    this.body = Some(v.into());
                }

                _ => {}
            }
        }

        Ok(this)
    }
}

#[derive(Clone, Debug, Error, PartialEq)]
pub enum Error {
    #[error("Invalid scheme - expected `mailto:`, got `{0}`")]
    InvalidScheme(String),

    #[error("Invalid url")]
    InvalidUrl(ParseError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_case::test_case;

    struct TestCase {
        given: &'static str,
        expected: fn() -> Mailto,
    }

    const TEST_TO: TestCase = TestCase {
        given: "mailto:jimmy@proton",
        expected: || Mailto {
            to: vec!["jimmy@proton".into()],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };

    const TEST_TO_ENCODED: TestCase = TestCase {
        given: "mailto:encoded%25jimmy@proton",
        expected: || Mailto {
            to: vec!["encoded%jimmy@proton".into()],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };

    const TEST_TO_QUERY_1: TestCase = TestCase {
        given: "mailto:?to=jimmy@proton",
        expected: || Mailto {
            to: vec!["jimmy@proton".into()],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };

    const TEST_TO_QUERY_2: TestCase = TestCase {
        given: "mailto:@?to=jimmy@proton",
        expected: || Mailto {
            to: vec!["jimmy@proton".into()],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };

    const TEST_TWO_TO: TestCase = TestCase {
        given: "mailto:jimmy@proton,kim@proton",
        expected: || Mailto {
            to: vec!["jimmy@proton".into(), "kim@proton".into()],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };

    const TEST_TWO_TO_SPACE: TestCase = TestCase {
        given: "mailto: jimmy@proton ,  kim@proton",
        expected: || Mailto {
            to: vec!["jimmy@proton".into(), "kim@proton".into()],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };

    const TEST_TWO_TO_QUERY: TestCase = TestCase {
        given: "mailto:jimmy@proton?to=kim@proton",
        expected: || Mailto {
            to: vec!["jimmy@proton".into(), "kim@proton".into()],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };

    const TEST_CC: TestCase = TestCase {
        given: "mailto:?cc=jimmy@proton",
        expected: || Mailto {
            to: vec![],
            cc: vec!["jimmy@proton".into()],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };

    const TEST_TO_AND_CC: TestCase = TestCase {
        given: "mailto:jimmy@proton?cc=kim@proton",
        expected: || Mailto {
            to: vec!["jimmy@proton".into()],
            cc: vec!["kim@proton".into()],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };

    const TEST_BCC: TestCase = TestCase {
        given: "mailto:?bcc=jimmy@proton",
        expected: || Mailto {
            to: vec![],
            cc: vec![],
            bcc: vec!["jimmy@proton".into()],
            subject: None,
            body: None,
        },
    };

    const TEST_TO_AND_BCC: TestCase = TestCase {
        given: "mailto:jimmy@proton?bcc=kim@proton",
        expected: || Mailto {
            to: vec!["jimmy@proton".into()],
            cc: vec![],
            bcc: vec!["kim@proton".into()],
            subject: None,
            body: None,
        },
    };

    const TEST_SUBJECT: TestCase = TestCase {
        given: "mailto:?subject=hello%20world",
        expected: || Mailto {
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: Some("hello world".into()),
            body: None,
        },
    };

    const TEST_BODY: TestCase = TestCase {
        given: "mailto:?body=hello%20world",
        expected: || Mailto {
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: Some("hello world".into()),
        },
    };

    const TEST_SUBJECT_AND_BODY: TestCase = TestCase {
        given: "mailto:?subject=hello&body=world",
        expected: || Mailto {
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: Some("hello".into()),
            body: Some("world".into()),
        },
    };

    const TEST_EVERYTHING: TestCase = TestCase {
        given: "mailto:jimmy@proton?cc=kim@proton&bcc=mike@proton&subject=hello&body=world",
        expected: || Mailto {
            to: vec!["jimmy@proton".into()],
            cc: vec!["kim@proton".into()],
            bcc: vec!["mike@proton".into()],
            subject: Some("hello".into()),
            body: Some("world".into()),
        },
    };

    const TEST_MIXED_CASE: TestCase = TestCase {
        given: "Mailto:?Body=hello%20world",
        expected: || Mailto {
            to: vec![],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: Some("hello world".into()),
        },
    };

    const TEST_MIXED_CASE2: TestCase = TestCase {
        given: "Mailto:hello@proton.com",
        expected: || Mailto {
            to: vec!["hello@proton.com".into()],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };

    const TEST_TWO_CC: TestCase = TestCase {
        given: "mailto:alice@example.com?cc=jimmy@proton,kim@proton",
        expected: || Mailto {
            to: vec!["alice@example.com".into()],
            cc: vec!["jimmy@proton".into(), "kim@proton".into()],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };

    const TEST_TWO_BCC: TestCase = TestCase {
        given: "mailto:alice@example.com?bcc=jimmy@proton,kim@proton",
        expected: || Mailto {
            to: vec!["alice@example.com".into()],
            cc: vec![],
            bcc: vec!["jimmy@proton".into(), "kim@proton".into()],
            subject: None,
            body: None,
        },
    };
    const TEST_TO_DELIMITED_BY_SEMICOLON: TestCase = TestCase {
        given: "mailto:jimmy@proton;kim@proton",
        expected: || Mailto {
            to: vec!["jimmy@proton".into(), "kim@proton".into()],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };
    const TEST_CC_DELIMITED_BY_SEMICOLON: TestCase = TestCase {
        given: "mailto:alice@proton?cc=jimmy@proton;kim@proton",
        expected: || Mailto {
            to: vec!["alice@proton".into()],
            cc: vec!["jimmy@proton".into(), "kim@proton".into()],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };
    const TEST_BCC_DELIMITED_BY_SEMICOLON: TestCase = TestCase {
        given: "mailto:alice@proton?bcc=jimmy@proton;kim@proton",
        expected: || Mailto {
            to: vec!["alice@proton".into()],
            cc: vec![],
            bcc: vec!["jimmy@proton".into(), "kim@proton".into()],
            subject: None,
            body: None,
        },
    };
    const TEST_TO_MIXED_WHITESPACE: TestCase = TestCase {
        given: "mailto:alice@proton,%20jimmy@proton;  kim@proton",
        expected: || Mailto {
            to: vec![
                "alice@proton".into(),
                "jimmy@proton".into(),
                "kim@proton".into(),
            ],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };

    const TEST_TO_DISPLAY_NAME: TestCase = TestCase {
        given: "mailto:%22Alice%20Example%22%20%3Calice@example.com%3E",
        expected: || Mailto {
            to: vec!["alice@example.com".into()],
            cc: vec![],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };
    const TEST_CC_DISPLAY_NAME: TestCase = TestCase {
        given: "mailto:bob@proton?cc=%22Alice%20Example%22%20%3Calice@example.com%3E",
        expected: || Mailto {
            to: vec!["bob@proton".into()],
            cc: vec!["alice@example.com".into()],
            bcc: vec![],
            subject: None,
            body: None,
        },
    };
    const TEST_BCC_DISPLAY_NAME: TestCase = TestCase {
        given: "mailto:bob@proton?bcc=%22Alice%20Example%22%20%3Calice@example.com%3E",
        expected: || Mailto {
            to: vec!["bob@proton".into()],
            cc: vec![],
            bcc: vec!["alice@example.com".into()],
            subject: None,
            body: None,
        },
    };

    #[allow(clippy::needless_pass_by_value)]
    #[test_case(TEST_TO)]
    #[test_case(TEST_TO_ENCODED)]
    #[test_case(TEST_TO_QUERY_1)]
    #[test_case(TEST_TO_QUERY_2)]
    #[test_case(TEST_TWO_TO)]
    #[test_case(TEST_TWO_TO_SPACE)]
    #[test_case(TEST_TWO_TO_QUERY)]
    #[test_case(TEST_CC)]
    #[test_case(TEST_TO_AND_CC)]
    #[test_case(TEST_BCC)]
    #[test_case(TEST_TO_AND_BCC)]
    #[test_case(TEST_SUBJECT)]
    #[test_case(TEST_BODY)]
    #[test_case(TEST_SUBJECT_AND_BODY)]
    #[test_case(TEST_EVERYTHING)]
    #[test_case(TEST_MIXED_CASE)]
    #[test_case(TEST_MIXED_CASE2)]
    #[test_case(TEST_TWO_CC)]
    #[test_case(TEST_TWO_BCC)]
    #[test_case(TEST_TO_DELIMITED_BY_SEMICOLON)]
    #[test_case(TEST_CC_DELIMITED_BY_SEMICOLON)]
    #[test_case(TEST_BCC_DELIMITED_BY_SEMICOLON)]
    #[test_case(TEST_TO_MIXED_WHITESPACE)]
    #[test_case(TEST_TO_DISPLAY_NAME)]
    #[test_case(TEST_CC_DISPLAY_NAME)]
    #[test_case(TEST_BCC_DISPLAY_NAME)]
    fn from_str(case: TestCase) {
        let actual = Mailto::from_str(case.given).unwrap();

        assert_eq!((case.expected)(), actual);
    }

    #[test]
    fn from_str_err_invalid_scheme() {
        let actual = Mailto::from_str("http://jimmy.com").unwrap_err();

        assert_eq!(Error::InvalidScheme("http".into()), actual);
    }

    #[test]
    fn from_str_err_invalid_url() {
        let actual = Mailto::from_str("http://").unwrap_err();

        assert_eq!(Error::InvalidUrl(ParseError::EmptyHost), actual);
    }
}
