use std::collections::HashMap;
use std::fmt::Debug;

use regex::Regex;

use crate::ParameterType;
use crate::errors::{VCardParameterError, VCardParameterResult};
use crate::values::check_list;
use crate::vcard::split_list;

/// The MEDIATYPE parameter is used with properties whose value is a URI.
#[derive(Debug, Clone)]
pub struct MediaType {
    /// type-name
    pub type_name: String,
    /// subtype-name
    pub subtype_name: String,
    /// attributes
    pub attributes: HashMap<String, String>,
}

impl MediaType {
    /// Create a new media-type parameter (doesn't check arguments validity)
    #[must_use]
    pub fn new_unchecked(
        type_name: &str,
        subtype: &str,
        attributes: HashMap<String, String>,
    ) -> Self {
        Self {
            type_name: type_name.to_owned(),
            subtype_name: subtype.to_owned(),
            attributes,
        }
    }

    /// Try to create a new media-type parameter from a str
    pub fn new_validated(value: &str) -> VCardParameterResult<Self> {
        Self::try_from(value)
    }
}

impl TryFrom<&[String]> for MediaType {
    type Error = VCardParameterError;

    fn try_from(values: &[String]) -> VCardParameterResult<Self> {
        if values.len() != 1 {
            return Err(VCardParameterError::ExpectedExactlyOneValue(
                ParameterType::MediaType,
                values.to_vec(),
            ));
        }
        Self::try_from(values[0].as_str())
    }
}

impl TryFrom<&str> for MediaType {
    type Error = VCardParameterError;

    fn try_from(value: &str) -> VCardParameterResult<Self> {
        fn error(value: &str) -> VCardParameterError {
            VCardParameterError::InvalidValue(ParameterType::MediaType, value.to_owned())
        }

        if let Some(position) = value.find(';') {
            if let Some((type_name, subtype_name)) = value[..position - 1].split_once('/') {
                if !is_reg_name(type_name) {
                    return Err(error(value));
                }
                if !is_reg_name(subtype_name) {
                    return Err(error(value));
                }
                let attributes = split_list(&value[position + 1..], ';');
                let attributes: HashMap<String, String> = attributes
                    .into_iter()
                    .map(|a| {
                        if let Some((n, v)) = a.split_once('=') {
                            Ok((n.to_owned(), v.to_owned()))
                        } else {
                            Err(error(value))
                        }
                    })
                    .collect::<Result<_, _>>()?;
                Ok(Self {
                    type_name: type_name.to_owned(),
                    subtype_name: subtype_name.to_owned(),
                    attributes,
                })
            } else {
                Err(error(value))
            }
        } else if let Some((type_name, subtype_name)) = value.split_once('/') {
            if !is_reg_name(type_name) {
                return Err(error(value));
            }
            if !is_reg_name(subtype_name) {
                return Err(error(value));
            }
            Ok(Self {
                type_name: type_name.to_owned(),
                subtype_name: subtype_name.to_owned(),
                attributes: HashMap::new(),
            })
        } else {
            Err(error(value))
        }
    }
}

fn is_reg_name(value: &str) -> bool {
    let re = Regex::new(r"^[a-zA-Z0-9!#$&.+-^_]{1,127}$").unwrap();
    re.is_match(value)
}

/// Validate that the given `values` respect the format for a `MEDIATYPE` parameter
pub fn is_mediatype_param(values: &[String]) -> bool {
    // mediatype-param = "MEDIATYPE=" mediatype
    // mediatype = type-name "/" subtype-name *( ";" attribute "=" value )
    //    ; "attribute" and "value" are from [RFC2045]
    //    ; "type-name" and "subtype-name" are from [RFC4288]
    //
    // From RFC4288:
    //   type-name = reg-name
    //   subtype-name = reg-name
    //   reg-name = 1*127reg-name-chars
    //   reg-name-chars = ALPHA / DIGIT / "!" / "#" / "$" / "&" / "." / "+" / "-" / "^" / "_"
    //
    // From RFC2045:
    //   attribute := token
    //                   ; Matching of attributes
    //                   ; is ALWAYS case-insensitive.
    //   value := token / quoted-string
    //   token := 1*<any (US-ASCII) CHAR except SPACE, CTLs, or tspecials>
    //   tspecials :=  "(" / ")" / "<" / ">" / "@" / "," / ";" / ":" / "\" / <"> / "/" / "[" / "]" / "?" / "="
    //                ; Must be in quoted-string,
    //                ; to use within parameter values
    //
    // From RFC822:
    //   quoted-string = <"> *(qtext/quoted-pair) <">; Regular qtext or
    //                                              ;   quoted chars.
    //
    //   qtext       =  <any CHAR excepting <">,     ; => may be folded
    //                  "\" & CR, and including
    //                  linear-white-space>
    //   quoted-pair =  "\" CHAR                     ; may quote any char
    //   CHAR        =  <any ASCII character>

    fn is_token(value: &str) -> bool {
        let re = Regex::new(r"[!#-'*+\x2D\x2E0-9?A-Z^-~]+").unwrap();
        re.is_match(value)
    }

    fn is_attribute_value(value: &str) -> bool {
        let re = Regex::new(r#""(\\.|[^\x22\\\r])*""#).unwrap();
        is_token(value) || re.is_match(value)
    }

    fn is_attribute(value: &str) -> bool {
        let Some((name, value)) = value.split_once('=') else {
            return false;
        };
        is_token(name) && is_attribute_value(value)
    }

    fn is_type(value: &str) -> bool {
        let Some((start, end)) = value.split_once('/') else {
            return false;
        };
        is_reg_name(start) && is_reg_name(end)
    }

    if values.len() == 1 {
        let value = values[0].as_str();
        if let Some(position) = value.find(';') {
            is_type(&value[..position - 1])
                && check_list(&value[position + 1..], is_attribute, ';').is_some()
        } else {
            is_type(value)
        }
    } else {
        false
    }
}
