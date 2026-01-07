use std::borrow::Cow;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::ops::Range;
use std::path::Path;
use std::str::FromStr;

use mail_parser::{Header, MimeHeaders};
use mail_parser::{Message, MessageParser};
use rand::RngCore;
use serde::{Deserialize, Serialize};

use crate::Disposition;
use crate::constants::mime_extensions;

/// Mime processing errors.
#[derive(Debug, thiserror::Error)]
pub enum ProcessMimeError {
    #[error("Mime parsing failed")]
    Parse,
    #[error("No body or attachment found in the mime message")]
    NoContent,
}

/// Function to transform a mime email message into a proton inbox message.
pub trait ProcessMime {
    /// Processes a decrypted mime body to a Proton inbox messages.
    ///
    /// Extracts the message body, extracts/normalizes the attachments, and collects the signatures.
    fn process_mime(message_id: &str, decrypted_body: &[u8]) -> ProcessedMimeResult;
}

/// An attachment extracted and from a mime message and processed.
#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
pub struct ProcessedAttachment {
    /// Unique id across all attachments in an inbox.
    pub id: String,

    /// Content id extracted from mime.
    pub content_id: String,

    /// Content disposition.
    pub disposition: Disposition,

    /// File name of the attachment.
    pub name: String,

    /// The size of the attachment in bytes.
    pub size: usize,

    /// The content type of the attachment.
    ///
    /// Is an empty string if no content type was found.
    pub mime_type: String,

    /// The attachment data.
    pub data: Vec<u8>,
}

/// Inbox message extracted from a mime message body.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct ProcessedMessage {
    /// The message body.
    pub body: String,

    /// The extracted attachments.
    pub attachments: Vec<ProcessedAttachment>,

    /// An extracted subject if any.
    pub encrypted_subject: Option<String>,

    /// The mime type of the extracted body.
    pub mime_body_type: ProcessedBodyType,

    /// The signatures extracted from the mime message.
    pub signatures: Vec<MimeSignatureVerifier>,
}

/// Represents a processed message body from a mime message.
#[derive(Debug, PartialEq, Eq, Clone, Hash, Copy)]
pub enum ProcessedBodyType {
    /// Utf-8 encoded text body,
    Text,

    /// HTML body.
    Html,

    /// Empty body.
    Empty,
}

impl ProcessedBodyType {
    /// Returns the mime type string.
    #[must_use]
    pub fn mime_type(&self) -> &str {
        match self {
            ProcessedBodyType::Text => "text/plain",
            ProcessedBodyType::Html => "text/html",
            ProcessedBodyType::Empty => "",
        }
    }
}

/// Represents a signature extracted from the mime message.
///
/// Contains the `OpenPGP` signature and the range of the raw data
/// the signatures has to be verified against.
#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct MimeSignatureVerifier {
    /// The range of the raw data that is signed.
    verify_data_range: Range<usize>,

    /// The `OpenPGP` signature of the data.
    pub pgp_signature: String,
}

impl MimeSignatureVerifier {
    /// Returns the a slice of the content that should be verified in `raw_data`.
    #[must_use]
    pub fn data_to_verify<'a>(&self, raw_data: &'a [u8]) -> &'a [u8] {
        &raw_data[self.verify_data_range.clone()]
    }
}

/// The result of a mime to inbox message transformation.
pub type ProcessedMimeResult = Result<ProcessedMessage, ProcessMimeError>;

/// Access to helper functions to transform mime messages into inbox messages.
pub struct MimeProcessor {}

impl ProcessMime for MimeProcessor {
    fn process_mime(message_id: &str, raw_data: &[u8]) -> ProcessedMimeResult {
        // Call the mime parsing library.
        let parsed_message = MessageParser::default()
            .parse(raw_data)
            .ok_or(ProcessMimeError::Parse)?;

        let (body, mime_body_type) = select_body(&parsed_message)?;
        // Process an normalize attachments for Proton.
        let processed_attachments = process_attachments(message_id, &parsed_message);
        // Extract signatures.
        let processed_signatures = process_signatures(&parsed_message);

        let encrypted_subject = parsed_message.subject().map(ToString::to_string);
        let processed_message = ProcessedMessage {
            body,
            attachments: processed_attachments,
            encrypted_subject,
            mime_body_type,
            signatures: processed_signatures,
        };

        Ok(processed_message)
    }
}

fn process_attachments(message_id: &str, parsed_message: &Message<'_>) -> Vec<ProcessedAttachment> {
    let mut processed_attachments = Vec::with_capacity(parsed_message.attachment_count());
    // Filename to counter map  for duplicate file names.
    let mut attachment_name_counter: HashMap<String, u32> = HashMap::new();
    processed_attachments.extend(parsed_message.attachments().enumerate().filter_map(
        |(idx, attachment)| {
            // Normalize each attachment
            let file_name_option = attachment.attachment_name();
            let content_type_option = attachment.content_type().map(|content_type| {
                let mut base_content_type = content_type.c_type.to_string();
                if let Some(sub_type) = content_type.subtype() {
                    base_content_type.push('/');
                    base_content_type.push_str(sub_type);
                }
                base_content_type
            });
            // Filter out signature attachments.
            if let Some(content_type) = &content_type_option
                && content_type == "application/pgp-signature"
            {
                return None;
            }
            // Generate a unique file name.
            let mut generated_filename =
                generate_file_name(file_name_option, content_type_option.as_ref());
            // Detect duplicated filenames and rename accordingly.
            let name_count_option = attachment_name_counter.get_mut(generated_filename.as_str());
            if let Some(name_count) = name_count_option {
                generated_filename = format!("{} ({})", generated_filename, *name_count);
                *name_count += 1;
            } else {
                attachment_name_counter.insert(generated_filename.clone(), 1);
            }

            // Use the existing content id or generate a random id.
            let content_id = attachment
                .content_id()
                .map_or_else(random_content_id, ToString::to_string);

            // Try to find disposition or default to attachment.
            let disposition =
                attachment
                    .content_disposition()
                    .map_or(Disposition::Attachment, |content_type| {
                        Disposition::from_str(content_type.ctype())
                            .unwrap_or(Disposition::Attachment)
                    });

            let data = attachment.contents().to_vec();

            Some(ProcessedAttachment {
                id: mime_attachment_id(message_id, &content_id, idx),
                content_id,
                disposition,
                name: generated_filename,
                size: data.len(),
                mime_type: content_type_option.unwrap_or(String::default()),
                data,
            })
        },
    ));
    processed_attachments
}

fn process_signatures(parsed_message: &Message<'_>) -> Vec<MimeSignatureVerifier> {
    parsed_message
        .parts
        .iter()
        .filter(|part| {
            // Filter the parts with the content type multipart/signed.
            let Some(sub_type) = extract_matched_content_subtype(part.headers(), "multipart")
            else {
                return false;
            };
            sub_type == "signed"
        })
        .filter_map(|signature_part| {
            // Parse the signature and determine the data to verify against.
            let sub_parts = signature_part.sub_parts()?;
            // There should be exactly two sub-parts: the body and the signature.
            if sub_parts.len() != 2 {
                return None;
            }
            // Determine the offsets in the raw body data to verify.
            let (offset_raw_start, offset_raw_end) = sub_parts
                .first()
                .and_then(|first| parsed_message.part(*first))
                .map(|body_part| (body_part.offset_header, body_part.offset_end))?;
            let signature_part = sub_parts
                .last()
                .and_then(|last| parsed_message.part(*last))?;
            // Check that the signature content type is application/pgp-signature.
            if extract_matched_content_subtype(signature_part.headers(), "application").is_none_or(
                |signature_content_type| signature_content_type.to_lowercase() != "pgp-signature",
            ) {
                return None;
            }
            // Extract the signature.
            let signature = signature_part.text_contents()?;

            Some(MimeSignatureVerifier {
                pgp_signature: signature.to_owned(),
                verify_data_range: Range {
                    start: offset_raw_start,
                    end: offset_raw_end,
                },
            })
        })
        .collect()
}

fn html_body_filter<'a>(parsed_message: &'a Message<'_>, idx: usize) -> Option<Cow<'a, str>> {
    let part = parsed_message.html_part(idx)?;
    let subtype = extract_matched_content_subtype(part.headers(), "text")?;
    if subtype != "html" {
        return None;
    }
    parsed_message.body_html(idx)
}

fn text_body_filter<'a>(parsed_message: &'a Message<'_>, idx: usize) -> Option<Cow<'a, str>> {
    let part = parsed_message.text_part(idx)?;
    let subtype = extract_matched_content_subtype(part.headers(), "text")?;
    if subtype != "plain" {
        return None;
    }
    parsed_message.body_text(idx)
}

fn select_body(
    parsed_message: &Message<'_>,
) -> Result<(String, ProcessedBodyType), ProcessMimeError> {
    if parsed_message.html_body_count() > 0 {
        // First priority are html bodies.
        // Concatenate all of them.
        const SPLIT_HTML: &str = "<br>\n";
        let mut total_body_size: usize = 0;
        let mut num_considered_bodies: usize = 0;
        (0..parsed_message.html_body_count())
            .filter_map(|idx| html_body_filter(parsed_message, idx))
            .for_each(|body_str| {
                total_body_size += body_str.len();
                num_considered_bodies += 1;
            });
        if num_considered_bodies > 0 {
            // Allocated the memory for the final body.
            let mut body = String::with_capacity(
                total_body_size + (num_considered_bodies - 1) * SPLIT_HTML.len(),
            );
            (0..parsed_message.html_body_count())
                .filter_map(|idx| html_body_filter(parsed_message, idx))
                .enumerate()
                .for_each(|(idx, data)| {
                    body.push_str(&data);
                    if idx != num_considered_bodies - 1 {
                        body.push_str(SPLIT_HTML);
                    }
                });
            body = body.replace("\r\n", "\n");
            return Ok((body, ProcessedBodyType::Html));
        }
    }
    if parsed_message.text_body_count() > 0 {
        // Second priority are text bodies.
        // Concatenate all of them.
        const SPLIT_TEXT: &str = "\n";
        let mut total_body_size: usize = 0;
        let mut num_considered_bodies: usize = 0;
        (0..parsed_message.text_body_count())
            .filter_map(|idx| text_body_filter(parsed_message, idx))
            .for_each(|body_str| {
                total_body_size += body_str.len();
                num_considered_bodies += 1;
            });
        if num_considered_bodies > 0 {
            // Allocated the memory for the final body.
            let mut body = String::with_capacity(
                total_body_size + (num_considered_bodies - 1) * SPLIT_TEXT.len(),
            );
            (0..parsed_message.text_body_count())
                .filter_map(|idx| text_body_filter(parsed_message, idx))
                .enumerate()
                .for_each(|(idx, data)| {
                    body.push_str(&data);
                    if idx != num_considered_bodies - 1 {
                        body.push_str(SPLIT_TEXT);
                    }
                });
            body = body.replace("\r\n", "\n");
            return Ok((body, ProcessedBodyType::Text));
        }
    }
    if parsed_message.attachment_count() == 0 {
        return Err(ProcessMimeError::NoContent);
    }
    // If there are attachments just use an empty body.
    Ok((String::new(), ProcessedBodyType::Empty))
}

const DEFAULT_FILE_NAME: &str = "attachment";

// Normalize parsed filename if present, otherwise generate a new one, trying to infer the file extension based
// on the provided content type.
fn generate_file_name(
    parsed_file_name_option: Option<&str>,
    content_type_option: Option<&String>,
) -> String {
    // The (old) MailParser used to return a generatedFileName, see https://github.com/nodemailer/mailparser/issues/238
    // now we generate it here instead, using a similar but simplified function  (e.g. we support fewer default extensions).
    if let Some(parsed_file_name) = parsed_file_name_option {
        // Remove the path if it is included in the filename
        let path = Path::new(parsed_file_name);
        path.file_name()
            .unwrap_or(OsStr::new(DEFAULT_FILE_NAME))
            .to_string_lossy()
            .to_string()
    } else if let Some(content_type) = content_type_option {
        mime_extensions()
            .get(content_type.as_str())
            .map_or(DEFAULT_FILE_NAME.to_owned(), |name| {
                format!("{DEFAULT_FILE_NAME}.{name}")
            })
    } else {
        DEFAULT_FILE_NAME.to_owned()
    }
}

// Generates a random content id.
fn random_content_id() -> String {
    let mut random_data: [u8; 16] = [0; 16];
    rand::thread_rng().fill_bytes(&mut random_data);
    format!("{}@pmcrypto>", hex::encode(random_data))
}

const ID_PREFIX: &str = "PGPAttachment";

// Creates a unique mime attachment id.
fn mime_attachment_id(api_message_id: &str, content_id: &str, number: usize) -> String {
    format!("{ID_PREFIX}_{api_message_id}_{content_id}_{number}")
}

/// Checks if the content type header is of type `content-type`
/// and returns its subtype if any. (type/subtype)
fn extract_matched_content_subtype<'a>(
    headers: &'a [Header<'a>],
    content_type: &str,
) -> Option<&'a str> {
    for header in headers {
        if let mail_parser::HeaderValue::ContentType(c) = header.value()
            && c.c_type == content_type
            && let Some(value) = &c.c_subtype
        {
            return Some(value.as_ref());
        }
    }
    None
}
