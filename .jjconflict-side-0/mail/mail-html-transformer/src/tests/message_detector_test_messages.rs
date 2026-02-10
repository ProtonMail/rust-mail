//! Test Emails
pub const DEFAULT: [(&str, &str); 16] = [
    ("proton_1", PROTON_1),
    ("proton_2", PROTON_2),
    ("gmail_1", GMAIL_1),
    ("gmail_2", GMAIL_2),
    ("gmail_3", GMAIL_3),
    ("gmail_4", GMAIL_4),
    ("gmx", GMX),
    ("android", ANDROID),
    ("aol_1", AOL_1),
    ("aol_2", AOL_2),
    ("icloud", ICLOUD), // MULTIPLE SAME LEVEL BLOCKQUOTE
    ("netease", NETEASE),
    ("sina", SINA),
    ("thunderbird", THUNDERBIRD),
    ("yahoo", YAHOO),
    ("zoho", ZOHO),
];

#[allow(dead_code)]
pub const UNSUPPORTED: [&str; 15] = [
    COMCAST,      // HR
    HOTMAIL_1,    // ALMOST NOTHING
    HOTMAIL_2,    // HR
    HOTMAIL_3,    // HR
    OUTLOOK_1,    // ??
    OUTLOOK_2,    // HR
    OUTLOOK_2003, // HR
    OUTLOOK_2007, // ALMOST NOTHING
    OUTLOOK_2010, // ALMOST NOTHING
    SPARROW,      // REALLY EXISTS?
    TENCENT,      // TEXT SEPARATOR + NOT INCLUDING
    WINDOWS_MAIL, // ALMOST NOTHING
    YANDEX_1,     // ONLY BLOCKQUOTE
    YANDEX_2,     // ONLY BLOCKQUOTE
    MAIL_RU,
];

// Collected by us.
pub const PROTON_1: &str = include_str!("./html/supported/proton_1.html");
pub const GMAIL_1: &str = include_str!("./html/supported/gmail_1.html");

// Mails found on https://github.com/mailgun/talon/tree/master/tests/fixtures
pub const ANDROID: &str = include_str!("./html/supported/android.html");
pub const AOL_1: &str = include_str!("./html/supported/aol_1.html");
pub const COMCAST: &str = include_str!("./html/unsupported/comcast.html");
pub const GMAIL_2: &str = include_str!("./html/supported/gmail_2.html");
pub const GMAIL_3: &str = include_str!("./html/supported/gmail_3.html");
pub const HOTMAIL_1: &str = include_str!("./html/unsupported/hotmail_1.html");
pub const HOTMAIL_2: &str = include_str!("./html/unsupported/hotmail_2.html");
pub const OUTLOOK_1: &str = include_str!("./html/unsupported/outlook_1.html");
pub const OUTLOOK_2003: &str = include_str!("./html/unsupported/outlook_2003.html");
pub const OUTLOOK_2007: &str = include_str!("./html/unsupported/outlook_2007.html");
pub const OUTLOOK_2010: &str = include_str!("./html/unsupported/outlook_2010.html");
pub const SPARROW: &str = include_str!("./html/unsupported/sparrow.html");
pub const MAIL_RU: &str = include_str!("./html/unsupported/mail_ru.html");
pub const THUNDERBIRD: &str = include_str!("./html/supported/thunderbird.html");
pub const WINDOWS_MAIL: &str = include_str!("./html/unsupported/windows_mail.html");
pub const YANDEX_1: &str = include_str!("./html/unsupported/yandex_1.html");

//Mails from https://github.com/felixfw1990/email-origin/tree/master/test/Providers
pub const AOL_2: &str = include_str!("./html/supported/aol_2.html");
pub const GMAIL_4: &str = include_str!("./html/supported/gmail_4.html");
pub const GMX: &str = include_str!("./html/supported/gmx.html");
pub const HOTMAIL_3: &str = include_str!("./html/unsupported/hotmail_3.html");
pub const ICLOUD: &str = include_str!("./html/supported/icloud.html");
pub const NETEASE: &str = include_str!("./html/supported/netease.html");
pub const OUTLOOK_2: &str = include_str!("./html/unsupported/outlook_2.html");
pub const PROTON_2: &str = include_str!("./html/supported/proton_2.html");
pub const SINA: &str = include_str!("./html/supported/sina.html");
pub const TENCENT: &str = include_str!("./html/unsupported/tencent.html");
pub const YAHOO: &str = include_str!("./html/supported/yahoo.html");
pub const YANDEX_2: &str = include_str!("./html/unsupported/yandex_2.html");
pub const ZOHO: &str = include_str!("./html/supported/zoho.html");
