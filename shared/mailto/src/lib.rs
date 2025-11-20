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

impl FromStr for Mailto {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let url = Url::from_str(s).map_err(Error::InvalidUrl)?;

        if url.scheme() != "mailto" {
            return Err(Error::InvalidScheme(url.scheme().into()));
        }

        let mut this = Self::default();

        // ---

        let to = url.path().trim();

        if !to.is_empty() && to != "@" {
            this.to = to.split(',').map(|to| to.trim().into()).collect();
        }

        // ---

        for (k, v) in url.query_pairs() {
            let k = k.trim();
            let v = v.trim().into();

            match k.trim() {
                k if k.eq_ignore_ascii_case("to") => {
                    this.to.push(v);
                }
                k if k.eq_ignore_ascii_case("cc") => {
                    this.cc.push(v);
                }
                k if k.eq_ignore_ascii_case("bcc") => {
                    this.bcc.push(v);
                }

                k if k.eq_ignore_ascii_case("subject") => {
                    this.subject = Some(v);
                }
                k if k.eq_ignore_ascii_case("body") => {
                    this.body = Some(v);
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

    #[allow(clippy::needless_pass_by_value)]
    #[test_case(TEST_TO)]
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
