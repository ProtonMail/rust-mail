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
//!   - **Attachment**
//!
//! NOTE: There is no streaming API at the moment.
use std::{
    fmt::{self, Display, Formatter},
    io::{self, Write},
};

use mail_builder::{
    headers::content_type::ContentType,
    mime::{BodyPart, MimePart},
};

const FILENAME: &str = "filename";
const NAME: &str = "name";
const MULTIPART_MIXED: &str = "multipart/mixed";
const MULTIPART_RELATED: &str = "multipart/related";
const MULTIPART_ALTERNATIVE: &str = "multipart/alternative";
const DEFAULT_MIME_TYPE_ATTACHMENT: &str = "application/octet-stream";
const MIME_TYPE_PLAIN: &str = "text/plain";
const MIME_TYPE_HTML: &str = "text/html";
const BODY_PLAIN_TRANSFER_ENCODING: &str = "quoted-printable";
const CONTENT_DISPOSITION_HEADER: &str = "Content-Disposition";

/// Possible dispositions of an attachment in the MIME builder.
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
enum Disposition {
    /// A regular attachment.
    Attachment,

    // An inline/embedded attachment.
    Inline,
}

impl Display for Disposition {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Disposition::Attachment => f.write_str("attachment"),
            Disposition::Inline => f.write_str("inline"),
        }
    }
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
///     .begin_html(br#"<html><body><h1>Hello</h1><img src="cid:image1"></body></html>"#)
///     
///     // Add an inline attachment (an image in this case)
///     // The image only belongs to the html body part and will not be considered in
///     // in the text plain only case.
///     .inline_attachment(
///         "image1",                   // Content-ID for the inline image
///         "example.png",              // Filename
///         Some("image/png"),          // MIME type
///         b"PNG image data"           // Content of the image
///     )
///     // Finalize the HTML part and return to the main builder
///     .end_html()
///     
///     //Add an attachment (a PDF file in this case)
///     .attachment(
///         "example.pdf",              // Filename
///         Some("application/pdf"),    // MIME type
///         b"%PDF-1.4 example data"    // Content of the file
///     )
///     
///     // Add an inline attachment (an image in this case)
///     .inline_attachment(
///         "image2",                   // Content-ID for the inline image
///         "2.png",                    // Filename
///         Some("image/png"),          // MIME type
///         b"PNG image data"           // Content of the image
///     )
///
///     // Write the generated MIME message to an output stream (e.g., a file or network stream)
///     .write_to(&mut output);
///
/// println!("{}",  std::str::from_utf8(&output).unwrap());
/// ```
pub struct InboxMimeBuilder<'x> {
    /// The text plain body part if any.
    text_body: Option<MimePart<'x>>,

    /// The html body part if any.
    html_body: Option<MimePart<'x>>,

    /// The attachments.
    attachments: Vec<MimePart<'x>>,
}

impl<'x> InboxMimeBuilder<'x> {
    /// Starts building a multipart MIME message.
    pub fn new() -> Self {
        Self {
            text_body: None,
            html_body: None,
            attachments: Vec::new(),
        }
    }

    /// Sets the plain text body of the email.
    ///
    /// # Parameters
    ///
    /// * `text_body` - The plain text body of the email.
    pub fn text_body(mut self, text_body: &'x str) -> Self {
        self.text_body = Some(
            MimePart::new(MIME_TYPE_PLAIN, BodyPart::Text(text_body.into()))
                .transfer_encoding("quoted-printable"),
        );
        self
    }

    /// Starts building the HTML body part of the message.
    ///
    /// # Parameters
    ///
    /// * `html_body` - The HTML body of the email.
    pub fn begin_html(self, html_body: &'x [u8]) -> HtmlMimeBuilder<'x> {
        HtmlMimeBuilder::new(
            self,
            MimePart::new(MIME_TYPE_HTML, BodyPart::Binary(html_body.into())),
        )
    }

    /// Adds an attachment to the message.
    ///
    /// # Parameters
    ///
    /// * `filename`  - The filename of the attachment.
    /// * `mime_type` - The MIME type of the attachment, if any.
    /// * `content`   - The content of the attachment.
    pub fn attachment(
        mut self,
        filename: &'x str,
        mime_type: Option<&'x str>,
        content: &'x [u8],
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
    ///
    /// # Parameters
    ///
    /// * `content_id`  - The content ID, used as a reference in the email body.
    /// * `filename`    - The filename of the attachment.
    /// * `mime_type`   - The MIME type of the attachment, if any.
    /// * `content`     - The content of the attachment.
    pub fn inline_attachment(
        mut self,
        content_id: &'x str,
        filename: &'x str,
        mime_type: Option<&'x str>,
        content: &'x [u8],
    ) -> Self {
        let part = create_attachment_mime_part(
            Disposition::Inline,
            Some(content_id),
            filename,
            mime_type,
            content,
        );
        self.attachments.push(part);
        self
    }

    /// Writes the multipart MIME message to the provided output writer.
    ///
    /// # Parameters
    ///
    /// * `output` - The output writer to which the data is written.
    ///
    /// # Errors
    /// Returns an error if writing to the output fails.
    pub fn write_to(self, output: impl Write) -> io::Result<()> {
        let mut parts = Vec::with_capacity(self.attachments.len() + 1);

        // Determine if the email has text and/or HTML content.
        let body_part = match (self.text_body, self.html_body) {
            (None, None) => MimePart::new("text/plain", BodyPart::Text("".into()))
                .transfer_encoding(BODY_PLAIN_TRANSFER_ENCODING),
            (None, Some(html_part)) => html_part,
            (Some(text_part), None) => text_part,
            (Some(text_part), Some(html_part)) => {
                MimePart::new(MULTIPART_ALTERNATIVE, vec![text_part, html_part])
            }
        };

        parts.push(body_part);
        parts.extend(self.attachments);

        // Create the final MIME message as multipart/mixed.
        MimePart::new(MULTIPART_MIXED, parts).write_part(output)?;

        Ok(())
    }
}

impl<'x> Default for InboxMimeBuilder<'x> {
    fn default() -> Self {
        Self::new()
    }
}

pub struct HtmlMimeBuilder<'x> {
    /// The message builder this builder belongs to.
    parent: InboxMimeBuilder<'x>,

    /// The HTML body.
    html_body: MimePart<'x>,

    /// All inline attachments only relevant to the HTML body.
    inline_attachments: Vec<MimePart<'x>>,
}

impl<'x> HtmlMimeBuilder<'x> {
    /// Starts building an HTML part.
    fn new(parent: InboxMimeBuilder<'x>, html_body: MimePart<'x>) -> Self {
        Self {
            parent,
            html_body,
            inline_attachments: Vec::new(),
        }
    }

    /// Adds an inline attachment that is only relevant in the HTML body.
    ///
    /// The inline attachment is considered relevant only to the HTML part and may be hidden in the text version.
    ///
    /// # Parameters
    ///
    /// * `content_id`  - The content ID, used as a reference in the email body.
    /// * `filename`    - The filename of the attachment.
    /// * `mime_type`   - The MIME type of the attachment, if any.
    /// * `content`     - The content of the attachment.
    pub fn inline_attachment(
        mut self,
        content_id: &'x str,
        filename: &'x str,
        mime_type: Option<&'x str>,
        content: &'x [u8],
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
    pub fn end_html(mut self) -> InboxMimeBuilder<'x> {
        let mut parts = Vec::with_capacity(self.inline_attachments.len() + 1);
        parts.push(self.html_body);
        parts.extend(self.inline_attachments);

        self.parent.html_body = Some(MimePart::new(MULTIPART_RELATED, parts));
        self.parent
    }
}

// Helper function to create a MimePart (either attachment or inline).
fn create_attachment_mime_part<'x>(
    disposition: Disposition,
    content_id: Option<&'x str>,
    filename: &'x str,
    mime_type: Option<&'x str>,
    content: &'x [u8],
) -> MimePart<'x> {
    let content_type = ContentType::new(mime_type.unwrap_or(DEFAULT_MIME_TYPE_ATTACHMENT))
        .attribute(FILENAME, filename)
        .attribute(NAME, filename);

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
