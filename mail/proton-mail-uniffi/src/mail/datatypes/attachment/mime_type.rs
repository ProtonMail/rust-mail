use proton_mail_common::datatypes::attachment::{
    MimeType as RealMimeType, MimeTypeCategory as RealMimeTypeCategory,
};
use uniffi::{Enum as UniffiEnum, Record as UniffiRecord};

#[derive(Debug, Clone, Copy, PartialEq, Eq, UniffiEnum, Hash)]
pub enum MimeTypeCategory {
    /// Audio MIME type category supports following MIME types:
    /// * "application/ogg",
    /// * "application/x-cdf",
    /// * "audio/aac",
    /// * "audio/aiff",
    /// * "audio/midi",
    /// * "audio/mpeg",
    /// * "audio/mpeg3",
    /// * "audio/ogg",
    /// * "audio/x-hx-aac-adts",
    /// * "audio/x-m4a",
    /// * "audio/x-midi",
    /// * "audio/x-mpeg-3",
    /// * "audio/x-realaudio",
    /// * "audio/x-wav",
    Audio,

    /// Calendar MIME type category supports following MIME types:
    /// * "text/calendar"
    Calendar,

    /// Code MIME type category supports following MIME types:
    /// * "application/atom+xml",
    /// * "application/javascript",
    /// * "application/json",
    /// * "application/ld+json",
    /// * "application/rss+xml",
    /// * "application/vnd.google-earth.kml+xml",
    /// * "application/x-csh",
    /// * "application/x-httpd-php",
    /// * "application/x-java-archive-diff",
    /// * "application/x-java-jnlp-file",
    /// * "application/x-perl",
    /// * "application/x-sh",
    /// * "application/x-tcl",
    /// * "application/xhtml+xml",
    /// * "application/xspf+xml",
    /// * "text/css",
    /// * "text/html",
    /// * "text/javascript",
    /// * "text/mathml",
    /// * "text/vnd.wap.wml",
    /// * "text/xml",
    Code,

    /// Compressed MIME type category supports following MIME types:
    /// * "application/gzip",
    /// * "application/java-archive",
    /// * "application/mac-binhex40",
    /// * "application/vnd.apple.installer+xml",
    /// * "application/vnd.google-earth.kmz",
    /// * "application/x-7z-compressed",
    /// * "application/x-bzip",
    /// * "application/x-bzip2",
    /// * "application/x-freearc",
    /// * "application/x-rar-compressed",
    /// * "application/x-tar",
    /// * "application/zip",
    Compressed,

    /// Default MIME type category is used when no other category is found
    /// but still defines following MIME types:
    /// * "application/epub+zip",
    /// * "application/octet-stream",
    /// * "application/vnd.amazon.ebook",
    /// * "application/vnd.mozilla.xul+xml",
    /// * "application/x-cocoa",
    /// * "application/x-makeself",
    /// * "application/x-pilot",
    /// * "application/x-redhat-package-manager",
    /// * "application/x-sea",
    /// * "application/x-shockwave-flash",
    /// * "application/x-stuffit",
    /// * "application/x-x509-ca-cert",
    /// * "application/x-xpinstall",
    /// * "text/vnd.sun.j2me.app-descriptor",
    Default,

    /// Excel MIME type category supports following MIME types:
    /// * "application/excel",
    /// * "application/vnd.ms-excel",
    /// * "application/vnd.oasis.opendocument.spreadsheet",
    /// * "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    /// * "application/x-excel",
    /// * "application/x-msexcel",
    Excel,

    /// Font MIME type category supports following MIME types:
    /// * "application/font-woff",
    /// * "application/vnd.ms-fontobject",
    /// * "font/otf",
    /// * "font/ttf",
    /// * "font/woff2",
    Font,

    /// Image MIME type category supports following MIME types:
    /// * "application/postscript",
    /// * "application/vnd.visio",
    /// * "image/gif",
    /// * "image/jpeg",
    /// * "image/jpg",
    /// * "image/png",
    /// * "image/svg+xml",
    /// * "image/tiff",
    /// * "image/vnd.wap.wbmp",
    /// * "image/webp",
    /// * "image/x-icon",
    /// * "image/x-jng",
    /// * "image/x-ms-bmp",
    /// * "video/x-mng",
    Image,

    /// Key MIME type category supports following MIME types:
    /// * "application/pgp-keys"
    Key,

    /// Keynote MIME type category supports following MIME types:
    /// * "application/vnd.apple.keynote",
    /// * "application/x-iwork-keynote-sffkey",
    Keynote,

    /// Numbers MIME type category supports following MIME types:
    /// * "application/vnd.apple.numbers",
    /// * "application/x-iwork-numbers-sffnumbers",
    Numbers,

    /// Pages MIME type category supports following MIME types:
    /// * "application/vnd.apple.pages",
    /// * "application/x-iwork-pages-sffpages",
    Pages,

    /// Pdf MIME type category supports following MIME types:
    /// * "application/pdf"
    Pdf,

    /// Powerpoint MIME type category supports following MIME types:
    /// * "application/mspowerpoint",
    /// * "application/powerpoint",
    /// * "application/vnd.ms-powerpoint",
    /// * "application/vnd.oasis.opendocument.presentation",
    /// * "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    /// * "application/x-mspowerpoint",
    /// * "text/x-pot",
    Powerpoint,

    /// Text MIME type category supports following MIME types:
    /// * "text/csv",
    /// * "text/plain",
    /// * "text/x-component"
    Text,

    /// Video MIME type category supports following MIME types:
    /// * "application/vnd.apple.mpegurl",
    /// * "application/vnd.wap.wmlc",
    /// * "image/mov",
    /// * "video/3gpp",
    /// * "video/avi",
    /// * "video/mp2t",
    /// * "video/mp4",
    /// * "video/mpeg",
    /// * "video/quicktime",
    /// * "video/webm",
    /// * "video/x-flv",
    /// * "video/x-m4v",
    /// * "video/x-matroska",
    /// * "video/x-ms-asf",
    /// * "video/x-ms-wmv",
    /// * "video/x-msvideo",
    /// * "video/x-quicktime",
    Video,

    /// Word MIME type category supports following MIME types:
    /// * "application/doc",
    /// * "application/ms-doc",
    /// * "application/msword",
    /// * "application/rtf",
    /// * "application/vnd.oasis.opendocument.text",
    /// * "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    /// * "application/x-abiword",
    Word,

    /// Unknown MIME type category is used when no other category is found
    Unknown,
}

impl From<RealMimeTypeCategory> for MimeTypeCategory {
    fn from(category: RealMimeTypeCategory) -> Self {
        match category {
            RealMimeTypeCategory::Audio => MimeTypeCategory::Audio,
            RealMimeTypeCategory::Calendar => MimeTypeCategory::Calendar,
            RealMimeTypeCategory::Code => MimeTypeCategory::Code,
            RealMimeTypeCategory::Compressed => MimeTypeCategory::Compressed,
            RealMimeTypeCategory::Default => MimeTypeCategory::Default,
            RealMimeTypeCategory::Excel => MimeTypeCategory::Excel,
            RealMimeTypeCategory::Font => MimeTypeCategory::Font,
            RealMimeTypeCategory::Image => MimeTypeCategory::Image,
            RealMimeTypeCategory::Key => MimeTypeCategory::Key,
            RealMimeTypeCategory::Keynote => MimeTypeCategory::Keynote,
            RealMimeTypeCategory::Numbers => MimeTypeCategory::Numbers,
            RealMimeTypeCategory::Pages => MimeTypeCategory::Pages,
            RealMimeTypeCategory::Pdf => MimeTypeCategory::Pdf,
            RealMimeTypeCategory::Powerpoint => MimeTypeCategory::Powerpoint,
            RealMimeTypeCategory::Text => MimeTypeCategory::Text,
            RealMimeTypeCategory::Video => MimeTypeCategory::Video,
            RealMimeTypeCategory::Word => MimeTypeCategory::Word,
            RealMimeTypeCategory::Unknown => MimeTypeCategory::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, UniffiRecord)]
pub struct AttachmentMimeType {
    /// The MIME type raw string. This string is used as a mean to communicate
    /// with rust backend about attachment mime types. It can also be consumed
    /// on client side but preffered way would be to inform about any additional
    /// features required so the calculations are done on the rust backend.
    pub mime: String,

    /// The MIME type category containing all of the known categories for icon
    /// choosing purposes.
    pub category: MimeTypeCategory,
}

impl From<RealMimeType> for AttachmentMimeType {
    fn from(mime: RealMimeType) -> Self {
        AttachmentMimeType {
            mime: mime.to_string(),
            category: mime.category().into(),
        }
    }
}
