use std::{collections::HashMap, sync::OnceLock};

pub fn mime_extensions() -> &'static HashMap<&'static str, &'static str> {
    static HASHMAP: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
    HASHMAP.get_or_init(|| {
        let mut m = HashMap::with_capacity(75);
        m.insert("application/octet-stream", "bin");
        m.insert("application/x-rar-compressed", "rar");
        m.insert("application/x-zip-compressed", "zip");
        m.insert("application/zip", "zip");
        m.insert("application/x-7z-compressed", "7z");
        m.insert("application/x-arj", "arj");
        m.insert("application/x-debian-package", "deb");
        m.insert("application/x-redhat-package-manager", "rpm");
        m.insert("application/x-rpm", "rpm");
        m.insert("application/vnd.rar", "rar");
        m.insert("application/gzip", "gz");
        m.insert("application/x-gzip", "gz");
        m.insert("application/x-compress", "z");
        m.insert("application/vnd.apple.installer+xml", "pkg");
        m.insert("application/msword", "doc");
        m.insert(
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            "docx",
        );
        m.insert("application/vnd.ms-powerpoint", "ppt");
        m.insert(
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
            "pptx",
        );
        m.insert(
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
            "xlsx",
        );
        m.insert("application/vnd.oasis.opendocument.spreadsheet", "ods");
        m.insert("application/vnd.oasis.opendocument.presentation", "odp");
        m.insert("application/xliff+xml", "xlf");
        m.insert("application/xml", "xml");
        m.insert("text/html", "html");
        m.insert("application/xhtml+xml", "xhtml");
        m.insert("application/pgp-keys", "asc");
        m.insert("application/rtf", "rtf");
        m.insert("application/x-tex", "tex");
        m.insert("application/vnd.oasis.opendocument.text", "odt");
        m.insert("application/vnd.wordperfect", "wpd");
        m.insert("application/vnd.ms-fontobject", "eot");
        m.insert("application/font-sfnt", "ttf");
        m.insert(
            "application/vnd.oasis.opendocument.formula-template",
            "odft",
        );
        m.insert("application/x-bzip", "bz");
        m.insert("application/x-bzip2", "bzip2");
        m.insert("application/epub+zip", "epub");
        m.insert("application/javascript", "js");
        m.insert("application/json", "json");
        m.insert("application/pdf", "pdf");
        m.insert("application/pgp-encrypted", "pgp");
        m.insert("application/pgp-signature", "asc");
        m.insert("application/pkcs7-mime", "p7m");
        m.insert("application/pkcs7-signature", "p7s");
        m.insert("audio/aac", "aac");
        m.insert("audio/midi", "midi");
        m.insert("audio/x-midi", "midi");
        m.insert("audio/ogg", "oga");
        m.insert("audio/mp3", "mp3");
        m.insert("audio/mp4", "m4a");
        m.insert("audio/mpeg", "mpga");
        m.insert("font/otf", "otf");
        m.insert("font/ttf", "ttf");
        m.insert("font/woff", "woff");
        m.insert("font/woff2", "woff2");
        m.insert("image/avif", "avif");
        m.insert("image/bmp", "bmp");
        m.insert("image/jpeg", "jpeg");
        m.insert("image/png", "png");
        m.insert("image/svg+xml", "svg");
        m.insert("image/tiff", "tif");
        m.insert("message/rfc822", "eml");
        m.insert("text/calendar", "ics");
        m.insert("text/css", "css");
        m.insert("text/csv", "csv");
        m.insert("text/markdown", "md");
        m.insert("text/plain", "txt");
        m.insert("text/richtext", "rtx");
        m.insert("text/vcard", "vcard");
        m.insert("text/xml", "xml");
        m.insert("text/yaml", "yaml");
        m.insert("video/x-msvideo", "avi");
        m.insert("video/mp4", "mp4");
        m.insert("video/mpeg", "mpeg");
        m.insert("video/quicktime", "mov");
        m.insert("video/webm", "webm");
        m.insert("video/ogg", "ogv");
        m
    })
}
