use crate::AppError;
use mime::Mime;
use stash::sql_using_serde;
use std::fmt;
use std::iter::repeat;
use std::sync::LazyLock;
use std::{collections::HashMap, str::FromStr};

pub type MimeName = &'static str;

macro_rules! zip {
    ($mime_types: expr, $mime_category: expr) => {
        $mime_types.into_iter().zip(repeat($mime_category))
    };
}

static MIME_MAP: LazyLock<HashMap<MimeName, MimeTypeCategory>> = LazyLock::new(|| {
    zip!(AUDIO_MIME_TYPES, MimeTypeCategory::Audio)
        .chain(zip!(CALENDAR_MIME_TYPES, MimeTypeCategory::Calendar))
        .chain(zip!(CODE_MIME_TYPES, MimeTypeCategory::Code))
        .chain(zip!(COMPRESSED_MIME_TYPE, MimeTypeCategory::Compressed))
        .chain(zip!(DEFAULT_MIME_TYPE, MimeTypeCategory::Default))
        .chain(zip!(EXCEL_MIME_TYPES, MimeTypeCategory::Excel))
        .chain(zip!(FONT_MIME_TYPE, MimeTypeCategory::Font))
        .chain(zip!(IMAGE_MIME_TYPE, MimeTypeCategory::Image))
        .chain(zip!(KEY_MIME_TYPE, MimeTypeCategory::Key))
        .chain(zip!(KEYNOTE_MIME_TYPE, MimeTypeCategory::Keynote))
        .chain(zip!(NUMBERS_MIME_TYPE, MimeTypeCategory::Numbers))
        .chain(zip!(PAGES_MIME_TYPE, MimeTypeCategory::Pages))
        .chain(zip!(PDF_MIME_TYPE, MimeTypeCategory::Pdf))
        .chain(zip!(POWERPOINT_MIME_TYPE, MimeTypeCategory::Powerpoint))
        .chain(zip!(TEXT_MIME_TYPE, MimeTypeCategory::Text))
        .chain(zip!(VIDEO_MIME_TYPE, MimeTypeCategory::Video))
        .chain(zip!(WORD_MIME_TYPE, MimeTypeCategory::Word))
        .map(|(mime, category)| (*mime, category))
        .collect()
});

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MimeTypeCategory {
    Audio,
    Calendar,
    Code,
    Compressed,
    Default,
    Excel,
    Font,
    Image,
    Key,
    Keynote,
    Numbers,
    Pages,
    Pdf,
    Powerpoint,
    Text,
    Video,
    Word,
    Unknown,
}

impl MimeTypeCategory {
    pub fn new<A: AsRef<str>>(mime: A) -> Self {
        let mime = mime.as_ref();

        MIME_MAP
            .get(mime)
            .cloned()
            .unwrap_or(MimeTypeCategory::Unknown)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MimeType {
    mime: Mime,
    category: MimeTypeCategory,
}

impl FromStr for MimeType {
    type Err = AppError;

    fn from_str(mime: &str) -> Result<Self, Self::Err> {
        let category = MimeTypeCategory::new(mime);
        let mime = mime.parse::<Mime>().map_err(|e| {
            AppError::InvalidMimeType(format!("`{mime}` could not be parsed, details: {e}"))
        })?;

        Ok(MimeType { mime, category })
    }
}

impl Default for MimeType {
    fn default() -> Self {
        // Best fit according to https://www.rfc-editor.org/rfc/rfc2046.txt
        MimeType {
            mime: mime::APPLICATION_OCTET_STREAM,
            category: MimeTypeCategory::Default,
        }
    }
}

impl MimeType {
    pub fn category(&self) -> MimeTypeCategory {
        self.category
    }
}

impl MimeType {
    pub fn text_html() -> Self {
        Self {
            mime: mime::TEXT_HTML,
            category: MimeTypeCategory::Code,
        }
    }

    pub fn text_plain() -> Self {
        Self {
            mime: mime::TEXT_PLAIN,
            category: MimeTypeCategory::Text,
        }
    }

    pub fn application_pdf() -> Self {
        Self {
            mime: mime::APPLICATION_PDF,
            category: MimeTypeCategory::Pdf,
        }
    }

    pub fn application_json() -> Self {
        Self {
            mime: mime::APPLICATION_JSON,
            category: MimeTypeCategory::Code,
        }
    }

    pub fn application_pgp_keys() -> Self {
        Self {
            mime: "application/pgp-keys".parse().expect("Should never fail"),
            category: MimeTypeCategory::Key,
        }
    }

    pub fn is_calendar(&self) -> bool {
        self.category == MimeTypeCategory::Calendar
    }
}

impl fmt::Display for MimeType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.mime)
    }
}

mod mime_deser {
    use super::*;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Deserialize, Serialize)]
    struct MimeTypeDeser {
        mime_type: String,
    }

    impl Serialize for MimeType {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            let mime = MimeTypeDeser {
                mime_type: self.mime.to_string(),
            };

            mime.serialize(serializer)
        }
    }

    impl<'de> Deserialize<'de> for MimeType {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let mime = MimeTypeDeser::deserialize(deserializer)?;

            mime.mime_type.parse().map_err(serde::de::Error::custom)
        }
    }
}

sql_using_serde!(MimeType);

const AUDIO_MIME_TYPES: &[MimeName] = &[
    "application/ogg",
    "application/x-cdf",
    "audio/aac",
    "audio/aiff",
    "audio/midi",
    "audio/mpeg",
    "audio/mpeg3",
    "audio/ogg",
    "audio/x-hx-aac-adts",
    "audio/x-m4a",
    "audio/x-midi",
    "audio/x-mpeg-3",
    "audio/x-realaudio",
    "audio/x-wav",
];

const CALENDAR_MIME_TYPES: &[MimeName] = &["text/calendar"];

const CODE_MIME_TYPES: &[MimeName] = &[
    "application/atom+xml",
    "application/javascript",
    "application/json",
    "application/ld+json",
    "application/rss+xml",
    "application/vnd.google-earth.kml+xml",
    "application/x-csh",
    "application/x-httpd-php",
    "application/x-java-archive-diff",
    "application/x-java-jnlp-file",
    "application/x-perl",
    "application/x-sh",
    "application/x-tcl",
    "application/xhtml+xml",
    "application/xspf+xml",
    "text/css",
    "text/html",
    "text/javascript",
    "text/mathml",
    "text/vnd.wap.wml",
    "text/xml",
];

const COMPRESSED_MIME_TYPE: &[MimeName] = &[
    "application/gzip",
    "application/java-archive",
    "application/mac-binhex40",
    "application/vnd.apple.installer+xml",
    "application/vnd.google-earth.kmz",
    "application/x-7z-compressed",
    "application/x-bzip",
    "application/x-bzip2",
    "application/x-freearc",
    "application/x-rar-compressed",
    "application/x-tar",
    "application/zip",
];

const DEFAULT_MIME_TYPE: &[MimeName] = &[
    "application/octet-stream",
    "application/epub+zip",
    "application/vnd.amazon.ebook",
    "application/vnd.mozilla.xul+xml",
    "application/x-cocoa",
    "application/x-makeself",
    "application/x-pilot",
    "application/x-redhat-package-manager",
    "application/x-sea",
    "application/x-shockwave-flash",
    "application/x-stuffit",
    "application/x-x509-ca-cert",
    "application/x-xpinstall",
    "text/vnd.sun.j2me.app-descriptor",
];

const EXCEL_MIME_TYPES: &[MimeName] = &[
    "application/excel",
    "application/vnd.ms-excel",
    "application/vnd.oasis.opendocument.spreadsheet",
    "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    "application/x-excel",
    "application/x-msexcel",
];

const FONT_MIME_TYPE: &[MimeName] = &[
    "application/font-woff",
    "application/vnd.ms-fontobject",
    "font/otf",
    "font/ttf",
    "font/woff2",
];

const IMAGE_MIME_TYPE: &[MimeName] = &[
    "application/postscript",
    "application/vnd.visio",
    "image/gif",
    "image/jpeg",
    "image/jpg",
    "image/png",
    "image/svg+xml",
    "image/tiff",
    "image/vnd.wap.wbmp",
    "image/webp",
    "image/x-icon",
    "image/x-jng",
    "image/x-ms-bmp",
    "video/x-mng",
];

const KEY_MIME_TYPE: &[MimeName] = &["application/pgp-keys"];

const KEYNOTE_MIME_TYPE: &[MimeName] = &[
    "application/vnd.apple.keynote",
    "application/x-iwork-keynote-sffkey",
];

const NUMBERS_MIME_TYPE: &[MimeName] = &[
    "application/vnd.apple.numbers",
    "application/x-iwork-numbers-sffnumbers",
];

const PAGES_MIME_TYPE: &[MimeName] = &[
    "application/vnd.apple.pages",
    "application/x-iwork-pages-sffpages",
];

const PDF_MIME_TYPE: &[MimeName] = &["application/pdf"];

const POWERPOINT_MIME_TYPE: &[MimeName] = &[
    "application/mspowerpoint",
    "application/powerpoint",
    "application/vnd.ms-powerpoint",
    "application/vnd.oasis.opendocument.presentation",
    "application/vnd.openxmlformats-officedocument.presentationml.presentation",
    "application/x-mspowerpoint",
    "text/x-pot",
];

const TEXT_MIME_TYPE: &[MimeName] = &["text/csv", "text/plain", "text/x-component"];

const VIDEO_MIME_TYPE: &[MimeName] = &[
    "application/vnd.apple.mpegurl",
    "application/vnd.wap.wmlc",
    "image/mov",
    "video/3gpp",
    "video/avi",
    "video/mp2t",
    "video/mp4",
    "video/mpeg",
    "video/quicktime",
    "video/webm",
    "video/x-flv",
    "video/x-m4v",
    "video/x-matroska",
    "video/x-ms-asf",
    "video/x-ms-wmv",
    "video/x-msvideo",
    "video/x-quicktime",
];

const WORD_MIME_TYPE: &[MimeName] = &[
    "application/doc",
    "application/ms-doc",
    "application/msword",
    "application/rtf",
    "application/vnd.oasis.opendocument.text",
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
    "application/x-abiword",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_map_len() {
        assert_eq!(MIME_MAP.len(), 129)
    }

    #[test]
    fn test_mime_structure() {
        for mime_str in MIME_MAP.keys() {
            let mime = mime_str.parse::<Mime>().unwrap();
            assert_eq!(&mime.to_string(), mime_str)
        }
    }

    #[test]
    fn test_deser_mime() {
        for mime_str in MIME_MAP.keys() {
            let expected_json = format!(r#"{{"mime_type":"{mime_str}"}}"#);
            let mime: MimeType = serde_json::from_str(&expected_json).unwrap();
            let actual = serde_json::to_string(&mime).unwrap();

            assert_eq!(actual, expected_json)
        }
    }
}
