use lazy_static::lazy_static;
use mime::Mime;
use stash::sql_using_serde;

use std::collections::HashMap;

pub type MimeName = &'static str;

lazy_static! {
    pub static ref MIME_MAP: HashMap<MimeName, MimeTypeCategory> = {
        let mut map = HashMap::new();
        for mime in AUDIO_MIME_TYPES {
            map.insert(*mime, MimeTypeCategory::Audio);
        }
        for mime in CALENDAR_MIME_TYPES {
            map.insert(*mime, MimeTypeCategory::Calendar);
        }
        for mime in CODE_MIME_TYPES {
            map.insert(*mime, MimeTypeCategory::Code);
        }
        for mime in COMPRESSED_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Compressed);
        }
        for mime in DEFAULT_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Default);
        }
        for mime in EXCEL_MIME_TYPES {
            map.insert(*mime, MimeTypeCategory::Excel);
        }
        for mime in FONT_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Font);
        }
        for mime in IMAGE_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Image);
        }
        for mime in KEY_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Key);
        }
        for mime in KEYNOTE_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Keynote);
        }
        for mime in NUMBERS_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Numbers);
        }
        for mime in PAGERS_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Pages);
        }
        for mime in PDF_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Pdf);
        }
        for mime in POWERPOINT_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Powerpoint);
        }
        for mime in TEXT_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Text);
        }
        for mime in VIDEO_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Video);
        }
        for mime in WORD_MIME_TYPE {
            map.insert(*mime, MimeTypeCategory::Word);
        }
        map
    };
}

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

impl MimeType {
    pub fn new<A: AsRef<str>>(mime: A) -> Result<Self, anyhow::Error> {
        let mime = mime.as_ref();
        let category = MimeTypeCategory::new(mime);
        let mime = mime
            .parse::<Mime>()
            .map_err(|e| anyhow::anyhow!("`{}` couldn not be parsed, details: {}", mime, e))?;

        Ok(MimeType { mime, category })
    }

    pub fn category(&self) -> MimeTypeCategory {
        self.category
    }

    pub fn text_html() -> Self {
        MimeType {
            mime: mime::TEXT_HTML,
            category: MimeTypeCategory::Text,
        }
    }

    pub fn text_plain() -> Self {
        MimeType {
            mime: mime::TEXT_PLAIN,
            category: MimeTypeCategory::Text,
        }
    }

    pub fn application_pdf() -> Self {
        MimeType {
            mime: mime::APPLICATION_PDF,
            category: MimeTypeCategory::Pdf,
        }
    }

    pub fn application_json() -> Self {
        MimeType {
            mime: mime::APPLICATION_JSON,
            category: MimeTypeCategory::Code,
        }
    }
}

impl std::fmt::Display for MimeType {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
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

            MimeType::new(mime.mime_type).map_err(serde::de::Error::custom)
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
    "application/epub+zip",
    "application/octet-stream",
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

const PAGERS_MIME_TYPE: &[MimeName] = &[
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
    use mime::Mime;

    use crate::datatypes::attachment::MimeType;

    #[test]
    fn test_mime_structure() {
        for mime_str in super::MIME_MAP.keys() {
            let mime = mime_str.parse::<Mime>().unwrap();
            assert_eq!(&mime.to_string(), mime_str)
        }
    }

    #[test]
    fn test_deser_mime() {
        for mime_str in super::MIME_MAP.keys() {
            let expected_json = format!(r#"{{"mime_type":"{}"}}"#, mime_str);
            let mime: MimeType = serde_json::from_str(&expected_json).unwrap();
            let actual = serde_json::to_string(&mime).unwrap();

            assert_eq!(actual, expected_json)
        }
    }
}
