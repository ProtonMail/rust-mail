mod properties;
mod values;
mod vcard;

#[cfg(test)]
pub fn validate_vcard(card: impl std::io::BufRead) {
    use ical::VcardParser;

    let card = VcardParser::new(card);
    for contact in card {
        use crate::vcard::VCard;

        _ = VCard::try_from(contact.expect("ical failed to parse vCard"))
            .expect("Failed to parse vCard");
    }
}

#[test]
fn vcard_full_proton() {
    // vCard generated using proton mail (web) on June 2024
    let vcard = r"BEGIN:VCARD
VERSION:4.0
FN;PREF=1:Foo Bar
PHOTO;PREF=1:https://www.publicdomainpictures.net/pictures/270000/t2/avatar
 -people-person-business-u-15354603894rE.jpg
PHOTO;PREF=2:https://www.publicdomainpictures.net/pictures/270000/t2/avatar
 -people-person-business-u-15354603894rE.jpg
LANG:Kingon
ROLE:The role
TITLE:The Title
TZ:UTC
N:Bar;Foo;;;
TEL;PREF=1:0123456789
TEL;TYPE=work;PREF=2:9876543210
ADR;PREF=1:;;42 avenue du tour;Paris;IdF;75022;France
ADR;TYPE=home;PREF=2:;;23 impasse du fond;Trou;Bretagne;01001;France
BDAY:20240522
NOTE:A very important note
NOTE:Another note
LOGO:https://www.publicdomainpictures.net/pictures/270000/t2/avatar-people-
 person-business-u-15354603894rE.jpg
MEMBER:uri:uri
ORG:The Organization
URL:https://www.publicdomainpictures.net/pictures/270000/t2/avatar-people-p
 erson-business-u-15354603894rE.jpg
GENDER:
ANNIVERSARY:20240522
UID:proton-web-f0453472-e174-e4cf-428a-a3d72c0a4c80
ITEM1.EMAIL;PREF=1:foo@bar.eu
ITEM2.EMAIL;TYPE=home;PREF=2:foo.bar@example.com
PRODID;VALUE=TEXT:-//ProtonMail//ProtonMail vCard 1.0.0//EN
ITEM2.CATEGORIES:Test Group
END:VCARD"
        .as_bytes();
    validate_vcard(vcard);
}

#[test]
fn vcard_many() {
    // Taken from RFC https://www.rfc-editor.org/rfc/rfc6350#section-6.6.5
    let vcard = r"BEGIN:VCARD
VERSION:4.0
KIND:group
FN:The Doe family
MEMBER:urn:uuid:03a0e51f-d1aa-4385-8a53-e29025acd8af
MEMBER:urn:uuid:b8767877-b4a1-4c70-9acc-505d3819e519
END:VCARD
BEGIN:VCARD
VERSION:4.0
FN:John Doe
UID:urn:uuid:03a0e51f-d1aa-4385-8a53-e29025acd8af
END:VCARD
BEGIN:VCARD
VERSION:4.0
FN:Jane Doe
UID:urn:uuid:b8767877-b4a1-4c70-9acc-505d3819e519
END:VCARD

BEGIN:VCARD
VERSION:4.0
KIND:group
FN:Funky distribution list
MEMBER:mailto:subscriber1@example.com
MEMBER:xmpp:subscriber2@example.com
MEMBER:sip:subscriber3@example.com
MEMBER:tel:+1-418-555-5555
END:VCARD"
        .as_bytes();
    validate_vcard(vcard);
}

#[test]
fn vcard_author() {
    // Taken from RFC https://www.rfc-editor.org/rfc/rfc6350#section-8
    let vcard = r#"BEGIN:VCARD
VERSION:4.0
FN:Simon Perreault
N:Perreault;Simon;;;ing. jr,M.Sc.
BDAY:--0203
ANNIVERSARY:20090808T1430-0500
GENDER:M
LANG;PREF=1:fr
LANG;PREF=2:en
ORG;TYPE=work:Viagenie
ADR;TYPE=work:;Suite D2-630;2875 Laurier;
 Quebec;QC;G1V 2M2;Canada
TEL;VALUE=uri;TYPE="work,voice";PREF=1:tel:+1-418-656-9254;ext=102
TEL;VALUE=uri;TYPE="work,cell,voice,video,text":tel:+1-418-262-6501
EMAIL;TYPE=work:simon.perreault@viagenie.ca
GEO;TYPE=work:geo:46.772673,-71.282945
KEY;TYPE=work;VALUE=uri:
 http://www.viagenie.ca/simon.perreault/simon.asc
TZ:-0500
URL;TYPE=home:http://nomis80.org
END:VCARD"#
        .as_bytes();
    validate_vcard(vcard);
}
