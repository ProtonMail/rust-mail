//! This module handles the construction of multipart MIME message bodies
//! for external PGP/MIME email communication. The generated MIME messages
//! consist of the following components:
//!
//! - Plaintext email body (mandatory)
//! - Optional HTML email body
//! - Embedded or inline attachments (such as images or media included in the email content)
//! - Regular attachments (files sent alongside the email)
//!
//! The builder follows the format of the Proton web client.
//! A built multipart MIME message has the following format:
//!
//! - **Mixed**
//!   - **Alternative**
//!     - **Text**
//!     - **Related**
//!       - **HTML**
//!       - **Inline Image**: HTML only
//!   - **Inline Image**: Relevant for all cases
//!   - **Attachment**
//!
//! NOTE: There is no streaming API at the moment.
use std::{
    borrow::Cow,
    io::{self, Write},
};

use mail_builder::{
    encoders::{base64::base64_encode_mime, quoted_printable::quoted_printable_encode},
    headers::content_type::ContentType,
    mime::{BodyPart, MimePart},
};

use crate::Disposition;

const FILENAME: &str = "filename";
const NAME: &str = "name";
const MULTIPART_MIXED: &str = "multipart/mixed";
const MULTIPART_RELATED: &str = "multipart/related";
const MULTIPART_ALTERNATIVE: &str = "multipart/alternative";
const DEFAULT_MIME_TYPE_ATTACHMENT: &str = "application/octet-stream";
const MIME_TYPE_PLAIN: &str = "text/plain";
const MIME_TYPE_HTML: &str = "text/html";
const QUOTED_PRINTABLE_ENCODING: &str = "quoted-printable";
const BASE_64_ENCODING: &str = "base64";
const CONTENT_DISPOSITION_HEADER: &str = "Content-Disposition";

/// Mime processing errors.
#[derive(Debug, thiserror::Error)]
pub enum BuildMimeError {
    #[error("Failed to encode: {0}")]
    Encode(&'static str),

    #[error("Failed to write: {0}")]
    Write(#[from] io::Error),
}

/// A builder for constructing multipart MIME message bodies for PGP/MIME.
///
/// The builder follows the format of the Proton web client.
/// A built multipart MIME message has the following format:
///
/// - **Mixed**
///   - **Alternative**
///     - **Text**
///     - **Related**
///       - **HTML**
///       - **Inline Image**: HTML only
///   - **Inline Image**: Images or media that are displayed within the body of the email.
///   - **Attachment**: Files that are sent alongside the email, but not displayed within the body (e.g., PDFs, images).
///
/// # Examples
///
/// ```
/// use proton_crypto_inbox_mime::write::InboxMimeBuilder;
/// let mut output = Vec::new();
///
/// InboxMimeBuilder::new()
///     // Add plain text body
///     .text_body("This is the plain text body of the email.")
///
///     // Begin the HTML part of the email
///     .begin_html_body(r#"<html><body><h1>Hello</h1><img src="cid:image1"></body></html>"#)
///     
///     // Add an inline attachment (an image in this case)
///     // The image only belongs to the html body part and will not be considered in
///     // in the text plain only case.
///     .inline_attachment(
///         "image1",                   // Content-ID for the inline image
///         "example.png",              // Filename
///         Some("image/png"),          // MIME type
///         b"PNG image data".to_vec()            // Content of the image
///     )
///     // Finalize the HTML part and return to the main builder
///     .end_html_body()
///     
///     //Add an attachment (a PDF file in this case)
///     .attachment(
///         "example.pdf",              // Filename
///         Some("application/pdf"),    // MIME type
///         b"%PDF-1.4 example data".to_vec()     // Content of the file
///     )
///     
///     // Add an inline attachment (an image in this case).
///     // Inline attachments do not need to be added exclusively to the HTML section.
///     // When added here, they will be available for both the HTML and plaintext versions of the email.
///     .inline_attachment(
///         "image2",                   // Content-ID for the inline image
///         "2.png",                    // Filename
///         Some("image/png"),          // MIME type
///         b"PNG image data".to_vec()           // Content of the image
///     )
///
///     // Write the generated MIME message to an output stream (e.g., a file or network stream)
///     .write_to(&mut output);
///
/// println!("{}",  std::str::from_utf8(&output).unwrap());
/// ```
pub struct InboxMimeBuilder<'x> {
    /// The text plain body part if any.
    text_body: Option<Result<MimePart<'x>, BuildMimeError>>,

    /// The html body part if any.
    html_body: Option<Result<MimePart<'x>, BuildMimeError>>,

    /// The attachments.
    attachments: Vec<MimePart<'x>>,
}

impl<'x> InboxMimeBuilder<'x> {
    /// Starts building a multipart MIME message.
    #[must_use]
    pub fn new() -> Self {
        Self {
            text_body: None,
            html_body: None,
            attachments: Vec::new(),
        }
    }

    /// Sets the plain text body of the email.
    #[must_use]
    pub fn text_body(mut self, text_body: &'x str) -> Self {
        self.text_body = Some(encode_text_plain_body(text_body));
        self
    }

    /// Sets the HTML body of the message without adding HTML-only inline attachments.
    ///
    /// If inline attachments are needed, use [`InboxMimeBuilder::begin_html_body`].
    #[must_use]
    pub fn html_body(self, html_body: &'x str) -> InboxMimeBuilder<'x> {
        self.begin_html_body(html_body).end_html_body()
    }

    /// Starts building the HTML body part of the message.
    ///
    /// Creates a builder that allows specifying HTML-specific inline attachments.
    /// Once the body is complete, call [`HtmlMimeBuilder::end_html_body`].
    #[must_use]
    pub fn begin_html_body(self, html_body: &'x str) -> HtmlBodyPartBuilder<'x> {
        let mut data = Vec::with_capacity(html_body.len());
        let Ok(_) = base64_encode_mime(html_body.as_bytes(), &mut data, false) else {
            return HtmlBodyPartBuilder::new(self, Err(BuildMimeError::Encode(BASE_64_ENCODING)));
        };
        let Ok(encoded) = String::from_utf8(data) else {
            return HtmlBodyPartBuilder::new(self, Err(BuildMimeError::Encode(BASE_64_ENCODING)));
        };
        HtmlBodyPartBuilder::new(
            self,
            Ok(
                MimePart::new(MIME_TYPE_HTML, BodyPart::Text(encoded.into()))
                    .transfer_encoding(BASE_64_ENCODING),
            ),
        )
    }

    /// Adds an attachment to the message.
    #[must_use]
    pub fn attachment(
        mut self,
        filename: &'x str,
        mime_type: Option<impl Into<Cow<'x, str>>>,
        content: Vec<u8>,
    ) -> Self {
        let part = create_attachment_mime_part(
            Disposition::Attachment,
            None,
            filename,
            mime_type,
            content,
        );
        self.attachments.push(part);
        self
    }

    /// Adds an inline attachment to the message, which is embedded in the email body.
    ///
    /// An inline attachment added here is still considered relevant to the email and will
    /// be displayed within the text. The builder also provides a method to add inline
    /// attachments specifically to the HTML part, in which case the inline image may only
    /// appear in the HTML version of the email and be hidden in the plain text version.
    #[must_use]
    pub fn inline_attachment(
        mut self,
        content_id: &'x str,
        filename: &'x str,
        mime_type: Option<impl Into<Cow<'x, str>>>,
        content: Vec<u8>,
    ) -> Self {
        // Ensure that the content-id is not in form <content-id>.
        let cleaned_content_id = content_id.trim_start_matches('<').trim_end_matches('>');
        let part = create_attachment_mime_part(
            Disposition::Inline,
            Some(cleaned_content_id),
            filename,
            mime_type,
            content,
        );
        self.attachments.push(part);
        self
    }

    /// Writes the multipart MIME message to the provided output writer.
    pub fn write_to(self, output: impl Write) -> Result<(), BuildMimeError> {
        let mut parts = Vec::with_capacity(self.attachments.len() + 1);

        let plain_body_part = self.text_body.transpose()?;

        // Determine if the email has text and/or HTML content.
        let body_part = match (plain_body_part, self.html_body) {
            (None, None) => MimePart::new("text/plain", BodyPart::Text("".into()))
                .transfer_encoding(QUOTED_PRINTABLE_ENCODING),
            (None, Some(html_part)) => html_part?,
            (Some(text_part), None) => text_part,
            (Some(text_part), Some(html_part)) => {
                MimePart::new(MULTIPART_ALTERNATIVE, vec![text_part, html_part?])
            }
        };

        parts.push(body_part);
        parts.extend(self.attachments);

        // Create the final MIME message as multipart/mixed.
        MimePart::new(MULTIPART_MIXED, parts).write_part(output)?;

        Ok(())
    }
}

/// Encodes the text body as quoted-printable.
fn encode_text_plain_body(text_body: &str) -> Result<MimePart<'_>, BuildMimeError> {
    let mut encoded_data = Vec::with_capacity(text_body.len());
    quoted_printable_encode(text_body.as_bytes(), &mut encoded_data, false, true)
        .map_err(|_| BuildMimeError::Encode(QUOTED_PRINTABLE_ENCODING))?;
    // The output is utf-8 encoded, thus, no error.
    let encoded_plain_body = String::from_utf8(encoded_data)
        .map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "read non-utf8 compliant quoted-printable data",
            )
        })
        .map_err(|_| BuildMimeError::Encode(QUOTED_PRINTABLE_ENCODING))?;
    Ok(
        MimePart::new(MIME_TYPE_PLAIN, BodyPart::Text(encoded_plain_body.into()))
            .transfer_encoding(QUOTED_PRINTABLE_ENCODING),
    )
}

impl Default for InboxMimeBuilder<'_> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct HtmlBodyPartBuilder<'x> {
    /// The message builder this builder belongs to.
    parent: InboxMimeBuilder<'x>,

    /// The HTML body.
    html_body: Result<MimePart<'x>, BuildMimeError>,

    /// All inline attachments only relevant to the HTML body.
    inline_attachments: Vec<MimePart<'x>>,
}

impl<'x> HtmlBodyPartBuilder<'x> {
    /// Starts building an HTML part.
    fn new(parent: InboxMimeBuilder<'x>, html_body: Result<MimePart<'x>, BuildMimeError>) -> Self {
        Self {
            parent,
            html_body,
            inline_attachments: Vec::new(),
        }
    }

    /// Adds an inline attachment that is only relevant in the HTML body.
    ///
    /// The inline attachment is considered relevant only to the HTML part and may be hidden in the text version.
    #[must_use]
    pub fn inline_attachment(
        mut self,
        content_id: &'x str,
        filename: &'x str,
        mime_type: Option<impl Into<Cow<'x, str>>>,
        content: Vec<u8>,
    ) -> Self {
        let part = create_attachment_mime_part(
            Disposition::Inline,
            Some(content_id),
            filename,
            mime_type,
            content,
        );
        self.inline_attachments.push(part);
        self
    }

    /// Finalizes the HTML body part.
    #[must_use]
    pub fn end_html_body(mut self) -> InboxMimeBuilder<'x> {
        let mut parts = Vec::with_capacity(self.inline_attachments.len() + 1);

        if let Ok(html_body) = self.html_body {
            parts.push(html_body);
            parts.extend(self.inline_attachments);
            self.parent.html_body = Some(Ok(MimePart::new(MULTIPART_RELATED, parts)));
        }
        self.parent
    }
}

// Helper function to create a MimePart (either attachment or inline).
fn create_attachment_mime_part<'x>(
    disposition: Disposition,
    content_id: Option<&'x str>,
    filename: &'x str,
    mime_type: Option<impl Into<Cow<'x, str>>>,
    content: Vec<u8>,
) -> MimePart<'x> {
    let content_type = if let Some(mime_type) = mime_type {
        ContentType::new(mime_type)
            .attribute(FILENAME, filename)
            .attribute(NAME, filename)
    } else {
        ContentType::new(DEFAULT_MIME_TYPE_ATTACHMENT)
            .attribute(FILENAME, filename)
            .attribute(NAME, filename)
    };

    let mut part = MimePart::new(content_type, content);
    part.headers.push((
        CONTENT_DISPOSITION_HEADER.into(),
        ContentType::new(disposition.to_string())
            .attribute(FILENAME, filename)
            .attribute(NAME, filename)
            .into(),
    ));

    if let Some(cid) = content_id {
        part = part.cid(cid);
    }

    part
}
