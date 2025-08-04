//! As compared to other tests, this one uses hardcoded fixtures, ones that were
//! exported from Proton Calendar.
//!
//! If you need to update them, it's as easy as:
//!
//! - exporting the address key (directly from settings),
//! - exporting the calendar key (via `GET /api/calendar/v2/bootstrap`),
//! - exporting the encrypted event data (via `GET /api/calendar/v1/.../events`).
//!
//! Remember to do this on the test environment to avoid leaking your actual
//! private keys.

use indoc::indoc;
use pretty_assertions as pa;
use proton_calendar_api::{
    CalendarBootstrap, CalendarKey, CalendarKeyFlags, CalendarMember, CalendarMemberPassphrase,
    CalendarPassphrase,
};
use proton_crypto::{
    crypto::{DataEncoding, PGPProviderSync},
    new_pgp_provider,
};
use proton_crypto_account::keys::{DecryptedAddressKey, KeyFlag, KeyId, UnlockedAddressKeys};
use proton_crypto_calendar::{
    CalendarEventDecryptor, EncryptedIcsRef, KeyPacketRef, KeyPackets, LockedCalendarKey,
    SignatureRef,
};

const ADDRESS_KEY: &str = indoc! {"
    -----BEGIN PGP PRIVATE KEY BLOCK-----

    xYYEaB21rhYJKwYBBAHaRw8BAQdAz9fj8ypGy0lr9ZzpPInkZs38O7zANJHw
    fcYuR60BgV/+CQMIsPRhDGNj+nFgAAAAAAAAAAAAAAAAAAAAAHk+sY7tEQ8r
    Ttvg251hiG4zfGs/JLxtxduZtjAfc3u/aoiUR1jXtcT7e+gubU8ZilHaUQ4h
    8c0pcHd5MTIzQHByb3Rvbi5ibGFjayA8cHd5MTIzQHByb3Rvbi5ibGFjaz7C
    wBEEExYKAIMFgmgdta4DCwkHCZD+wpExkdx9YkUUAAAAAAAcACBzYWx0QG5v
    dGF0aW9ucy5vcGVucGdwanMub3JnAN02vLR8KrW+FYcTHosEhyz8Ip2J2147
    5L62kBdn1lQDFQoIBBYAAgECGQECmwMCHgEWIQQxwRPz2qOmi81oFV3+wpEx
    kdx9YgAA1DQA/0B9tN/stm5V0v1jtwAjgdbOdPBM92EUE7zDgYGMwi8WAP9g
    WgGiZEU+lAGt8xM/1j4SSnlXhHRJevV0ICKUDHXRCseLBGgdta4SCisGAQQB
    l1UBBQEBB0DDGqunemwmJL16VrsBWDhzfToDSKq1039K3jOzeUhWZQMBCAf+
    CQMImJGpOr+7DqRgAAAAAAAAAAAAAAAAAAAAADAiOyzamlZqwwWHs1JHQLcu
    CbJ4mbXhPeE64+wIBGfbNz7co63aRwiDLiuMRtcx+QjEoeQhkMK+BBgWCgBw
    BYJoHbWuCZD+wpExkdx9YkUUAAAAAAAcACBzYWx0QG5vdGF0aW9ucy5vcGVu
    cGdwanMub3JnHamJ2Xujj6vyI4eAsBar0LcANOZAsAvpzpEkaVNvcTECmwwW
    IQQxwRPz2qOmi81oFV3+wpExkdx9YgAAws4A/RAm4Z//yrEzJs9cNz0YPwM5
    lsrZ/IfKmWEO3fwkbxgzAQDqDFCQ9vFzux6rTW/DzT3AJ1FEWNRcapktsAT6
    feq5Bw==
    =hAV1
    -----END PGP PRIVATE KEY BLOCK-----
"};

const ADDRESS_KEY_PASS: &str = "verysecret";

const CALENDAR_KEY: &str = indoc! {"
    -----BEGIN PGP PRIVATE KEY BLOCK-----
    Version: ProtonMail

    xYYEaB27dBYJKwYBBAHaRw8BAQdAftHrvASEwWP7eSjdC9ehbU2UmP7ce4aG
    nibMAZr+prb+CQMI6+MCIBmqTFFgAAAAAAAAAAAAAAAAAAAAALx6TtEpHrP7
    siDxfR4U08jTjpwG25hu4H/jP956SzRG/8H5jl/TqTWt+ZjcUPn4CXuiofWy
    gM0MQ2FsZW5kYXIga2V5wsARBBMWCgCDBYJoHbt0AwsJBwmQ9xVg81M5J1pF
    FAAAAAAAHAAgc2FsdEBub3RhdGlvbnMub3BlbnBncGpzLm9yZ4SUBpMyZrSH
    E1DaNwp62TqURz7dzorfGHPdw1RnlFbOAxUKCAQWAAIBAhkBApsDAh4BFiEE
    pU9LuBXctAvjCUi79xVg81M5J1oAACfVAQCK/JFR05M0Zw+j3atOa3t/U44O
    XUKSuLE6yQYroHzZgAEAl5vLQyQzuY77GUa/vSZbyRY6ZLg3QoboXMpUpDDL
    uwLHiwRoHbt0EgorBgEEAZdVAQUBAQdAlxB3IpQncDsXihCaglf8xS6P9cdK
    NsVXGN3vjHdZPX4DAQgH/gkDCCoRpBKuNH7NYAAAAAAAAAAAAAAAAAAAAAAd
    MiZ8IV7UNyKPUUDoRrg7keJzqAN4fIa11jmrnRuD6qrtZgmg68LFUdQnHSr4
    oDZeVl9VkuHCvgQYFgoAcAWCaB27dAmQ9xVg81M5J1pFFAAAAAAAHAAgc2Fs
    dEBub3RhdGlvbnMub3BlbnBncGpzLm9yZzEXWo8Aivfd0GijLcygfo4DhG0S
    zHI/jJ/0Jxj2PI8/ApsMFiEEpU9LuBXctAvjCUi79xVg81M5J1oAADqwAQDu
    4y6+E8Im6qi4CqWLkn22BOBORoxj4/mRKwBiMxJI6QEAkT4I6xI3+hcs5lyn
    InDufF9nOCr8QYrVo/FqkNWPAgA=
    =z7hW
    -----END PGP PRIVATE KEY BLOCK-----
"};

const CALENDAR_KEY_PASS: &str = indoc! {"
    -----BEGIN PGP MESSAGE-----
    Version: ProtonMail

    wV4D15nniWuUiRwSAQdAPICejZKCixljXsDAVtKkIs4Km6oowfsRKMHSh9Jm
    SUAwuVMZ9yvUXwj+6fELiaTLeV0OG7LB18Cuh7UnAyzIpP14r+rte1Wy1Xvm
    nv/tFpaQ0l0Btss6FbTWpF1NPkl6w9R3WL8F2iEQr2AOTVsf5dKjCCKjEISP
    RHc7zuWuLwQ3NtFkasdER1fdJFGBgfpjuCGCRQgPeISzkwoOOZ2ZXuiJef8M
    h7t1fzxchXjcwSQ=
    =xfWX
    -----END PGP MESSAGE-----
"};

const CALENDAR_KEY_SIG: &str = indoc! {"
    -----BEGIN PGP SIGNATURE-----
    Version: ProtonMail

    wrsEARYKAG0Fgmgdu7AJkP7CkTGR3H1iRRQAAAAAABwAIHNhbHRAbm90YXRp
    b25zLm9wZW5wZ3Bqcy5vcmfD4+3N2Qwn5sDT0mBsbW6zvrn+SKP4SUo1EUCp
    vFbz1RYhBDHBE/Pao6aLzWgVXf7CkTGR3H1iAAB8UgEA8PFm2YHP2u8kPEEh
    D/BXAW7iF6UTB7QJ8cvuXrH1Ds4BAKn1WugC3wKWOCfxnJ7pQctjXk88wmzG
    WjbxFPHA9n8P
    =wL/c
    -----END PGP SIGNATURE-----
"};

const SHARED_KEY_PACKET: &str = indoc! {"
    wV4D4TeBEIg/PFUSAQdAZN7fYX7oUeFs81/XytcidIYaErZQru1aUCbNa0W3
    ZmkwzeskcNwtlUIX1Lp51ElT7pAlDluSOiSh5Ffmn1ohEeAsy79SPTpA8rwA
    xxCdkm9I
"};

const SHARED_EVENT_DATA: &str = indoc! {"
    0sB0AVJI7tGQm8RPE8vIVx5kopp5HfhZ+ZZ1KVHlU8LU4LHFv/dPXCmoejz/
    dw6bgujGSLFK57bRKhCK8JcyMGwCPol64QLWh9677ZJ78XD6LCkO2P9P07jQ
    t94MZUMeRp/KDZAYrM1xUyAWhVUJB2Tf5jnHoMnHOa87gRT2XDh+agXFOatY
    uLSI4Dj6uPiNfQYIVx0sIj1Jo2edjyfAGnY/YbNA8kUzK77k/W0KnPhmCtEZ
    cbccJKMPCj/sgl8tFQkpwhQY0XU8a354fo/wRYxE5saJ0PnKx1RA3MwRQYs+
    DEpdGR6wXU8xaOZk3AgNxRc7KsCI0/B8qs8alNCIqgKkaTQKE5eW599xts7A
    qJFJfZRkb1WV3lJmxPPO32cdPbUVJLZ4x5ar3DhijMvrJa1ngS+pUrA=
"};

const SHARED_EVENT_SIG: &str = indoc! {"
    -----BEGIN PGP SIGNATURE-----

    wrsEABYKAG0Fgmgdu7MJkP7CkTGR3H1iRRQAAAAAABwAIHNhbHRAbm90YXRp
    b25zLm9wZW5wZ3Bqcy5vcmefuKFxVz5ZaqN7CvAqWBisVl3zxoOS8CI6Xd7z
    zIi2ghYhBDHBE/Pao6aLzWgVXf7CkTGR3H1iAADHKwEAoZnCkEIOor4njUe5
    WDV/Hs6yVkVO+8NV3ZRc3rIyk9wA/RAN0Gi1azKlchT+6RhL+TVpVHH6ogfb
    ulSGp/8qlosC
    =PYAq
    -----END PGP SIGNATURE-----
"};

#[test]
fn smoke() {
    let pgp = new_pgp_provider();

    let address_keys = {
        let private_key = pgp
            .private_key_import(ADDRESS_KEY, ADDRESS_KEY_PASS, DataEncoding::Armor)
            .unwrap();

        let public_key = pgp.private_key_to_public_key(&private_key).unwrap();

        UnlockedAddressKeys::from(DecryptedAddressKey {
            id: KeyId(String::default()),
            flags: KeyFlag::default(),
            primary: true,
            is_v6: false,
            private_key,
            public_key,
        })
    };

    let calendar_key = LockedCalendarKey::from_bootstrap(&CalendarBootstrap {
        keys: vec![CalendarKey {
            id: "TNJPb5Qu".into(),
            private_key: CALENDAR_KEY.into(),
            flags: CalendarKeyFlags::ActiveAndPrimary,
        }],
        passphrase: CalendarPassphrase {
            member_passphrases: vec![CalendarMemberPassphrase {
                member_id: "MlGaYvPr".into(),
                passphrase: CALENDAR_KEY_PASS.into(),
                signature: CALENDAR_KEY_SIG.into(),
            }],
        },
        members: [CalendarMember {
            id: "MlGaYvPr".into(),
            name: "My calendar".into(),
            color: "#936D58".into(),
            address_id: "tGEBZWwt".into(),
        }],
    })
    .unwrap()
    .import(&pgp, &address_keys)
    .unwrap();

    let shared_key_packet = SHARED_KEY_PACKET.replace('\n', "");
    let shared_event_data = SHARED_EVENT_DATA.replace('\n', "");
    let shared_event_data = EncryptedIcsRef::from_base64(&shared_event_data);
    let shared_event_sig = SignatureRef::from_armored(SHARED_EVENT_SIG);

    let key_packets = KeyPackets {
        address_key_packet: None,
        shared_key_packet: Some(KeyPacketRef::from_base64(&shared_key_packet)),
    };

    let event = CalendarEventDecryptor::new(&pgp, &address_keys, &calendar_key, key_packets)
        .unwrap()
        .decrypt(&pgp, shared_event_data, Some(shared_event_sig))
        .unwrap();

    let actual = String::from_utf8(event.into_bytes()).unwrap();

    let expected = indoc! {"
        BEGIN:VCALENDAR
        VERSION:2.0
        PRODID:-//Proton AG//web-calendar 5.0.999.999//EN
        BEGIN:VEVENT
        UID:NdN8ErHVJhXvfcSR500zXiMq-0Co@proton.me
        DTSTAMP:20250509T082419Z
        DESCRIPTION:Let's Hello this World\\, pals!
        SUMMARY:Hello\\, World!
        END:VEVENT
        END:VCALENDAR
    "};

    // *.ics doesn't have trailing empty line and it uses `\r\n` line endings:
    let expected = expected.trim_end().replace('\n', "\r\n");

    pa::assert_eq!(expected, actual);
}
