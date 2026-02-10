use crate::vcard::VCard;
use ical::VcardParser;

#[test]
fn import_web() {
    // vcard as exported via proton-web with all field used
    let vcard = b"BEGIN:VCARD
VERSION:4.0
FN;PREF=1:Foo Bar
PHOTO;PREF=1:https://www.publicdomainpictures.net/pictures/270000/t2/avatar
 -people-person-business-u-15354603894rE.jpg
PHOTO;PREF=2:https://www.publicdomainpictures.net/pictures/270000/t2/avatar
 -people-person-business-u-15354603894rE.jpg
LANG:Klingon
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
ITEM2.EMAIL;TYPE=;PREF=2:foo.bar@example.com
PRODID;VALUE=TEXT:-//ProtonMail//ProtonMail vCard 1.0.0//EN
ITEM2.CATEGORIES:Test group,Other group
END:VCARD";
    let vcard = VcardParser::new(&vcard[..]);
    for card in vcard {
        let card = card.unwrap();
        VCard::try_from(card).unwrap();
    }
}

#[test]
fn import_full() {
    // vcard with all property and parameters
    let vcard = r#"BEGIN:VCARD
VERSION:4.0
GROUP1.ADR;VALUE=text;LABEL=foo;LANGUAGE=fr;GEO="uri:uri";TZ="uri:uri";ALTID=1;PID=1.2,3;PREF=1;X-any=bar:a,b;c,d;e,f;g,h;i,j;k,l;m,n
GROUP2.ADR;VALUE=text;LABEL=bar;LANGUAGE=en;GEO="uri:uri";TZ="uri:uri";ALTID=2;PID=1.2,3;PREF=2;IANA=foo:a,b;c,d;e,f;g,h;i,j;k,l;m,n
GROUP1.ANNIVERSARY;VALUE=date-and-or-time;ALTID=1;CALSCALE=gregorian;X-any=bar:19960415
GROUP2.BDAY;VALUE=text;ALTID=1;CALSCALE=gregorian;LANGUAGE=fr;IANA=foo:19960415
GROUP1.CALURI;VALUE=uri;PID=1.2,3;PREF=1;TYPE=work;MEDIATYPE=name/sub;attr=val;attr2=val2;ALTID=1;X-any=bar:uri:uri
GROUP2.CALURI;VALUE=uri;PID=1.2,3;PREF=2;TYPE=home;MEDIATYPE=name/sub;attr=val;attr2=val2;ALTID=2;IANA=foo:uri:uri
GROUP1.CALADRURI;VALUE=uri;PID=1.2,3;PREF=1;TYPE=work;MEDIATYPE=name/sub;ALTID=1;X-any=bar:uri:uri
GROUP2.CALADRURI;VALUE=uri;PID=1.2,3;PREF=2;TYPE=home;MEDIATYPE=name/sub;ALTID=2;IANA=foo:uri:uri
GROUP1.CATEGORIES;VALUE=text;PID=1.2,3;PREF=1;TYPE=work;ALTID=1;X-any=bar:First,Second
GROUP2.CATEGORIES;VALUE=text;PID=1.2,3;PREF=2;TYPE=home;ALTID=2;IANA=foo:Third,Fourth
GROUP1.CLIENTPIDMAP;X-any=bar:1;uri:uri
GROUP2.CLIENTPIDMAP;IANA=foo:2;uri:uri
GROUP1.EMAIL;VALUE=text;PID=1.2,3;PREF=1;TYPE=work;ALTID=1;X-any=bar:example@example.com
GROUP2.EMAIL;VALUE=text;PID=1.2,3;PREF=2;TYPE=home;ALTID=2;IANA=foo:example@example.com
GROUP1.FBURL;VALUE=uri;PID=1.2,3;PREF=1;TYPE=work;MEDIATYPE=type/sub;ALTID=1;X-any=bar:uri:uri
GROUP2.FBURL;VALUE=uri;PID=1.2,3;PREF=2;TYPE=home;MEDIATYPE=type/sub;ALTID=2;IANA=foo:uri:uri
GROUP1.FN;VALUE=text;TYPE=work;LANGUAGE=en;ALTID=1;PID=1.2,3;PREF=1;X-any=bar:Foo Bar
GROUP2.FN;VALUE=text;TYPE=home;LANGUAGE=fr;ALTID=2;PID=1.2,3;PREF=2;IANA=foo:Foo Bar
GENDER;VALUE=text;IANA=foo:;it's complicated
GROUP1.GEO;VALUE=uri;PID=1.2,3;PREF=1;TYPE=work;MEDIATYPE=type/sub;ALTID=1;X-any=bar:uri:uri
GROUP2.GEO;VALUE=uri;PID=1.2,3;PREF=2;TYPE=home;MEDIATYPE=type/sub;ALTID=2;IANA=foo:uri:uri
GROUP1.IMPP;VALUE=uri;PID=1.2,3;PREF=1;TYPE=work;MEDIATYPE=type/sub;ALTID=1;X-any=bar:uri:uri
GROUP2.IMPP;VALUE=uri;PID=1.2,3;PREF=2;TYPE=home;MEDIATYPE=type/sub;ALTID=2;IANA=foo:uri:uri
GROUP1.KEY;VALUE=uri;MEDIATYPE=type/sub;ALTID=1;PID=1.2,3;PREF=1;TYPE=work;X-any=bar:uri:uri
GROUP2.KEY;VALUE=text;ALTID=2;PID=1.2,3;PREF=2;TYPE=home;IANA=foo:text
KIND;VALUE=text;X-any=bar:individual
GROUP1.LANG;VALUE=language-tag;PID=1.2,3;PREF=1;ALTID=1;TYPE=work;X-any=bar:en
GROUP2.LANG;VALUE=language-tag;PID=1.2,3;PREF=2;ALTID=2;TYPE=home;IANA=foo:fr
GROUP1.LOGO;VALUE=uri;LANGUAGE=en;PID=1.2,3;PREF=1;TYPE=work;MEDIATYPE=type/sub;ALTID=1;X-any=bar:uri:uri
GROUP2.LOGO;VALUE=uri;LANGUAGE=fr;PID=1.2,3;PREF=2;TYPE=home;MEDIATYPE=type/sub;ALTID=2;IANA=foo:uri:uri
GROUP1.MEMBER;VALUE=uri;PID=1.2,3;PREF=1;ALTID=1;MEDIATYPE=type/sub;X-any=bar:uri:uri
GROUP2.MEMBER;VALUE=uri;PID=1.2,3;PREF=2;ALTID=2;MEDIATYPE=type/sub;IANA=foo:uri:uri
N;VALUE=text;SORT-AS=foo,bar;LANGUAGE=en;ALTID=1;X-any=bar:a,b;c,d;e,f;g,h;i,j
GROUP1.NICKNAME;VALUE=text;TYPE=work;LANGUAGE=en;ALTID=1;PID=1.2,3;PREF=1;X-any=bar:text
GROUP2.NICKNAME;VALUE=text;TYPE=home;LANGUAGE=fr;ALTID=2;PID=1.2,3;PREF=2;IANA=foo:text
GROUP1.NOTE;VALUE=text;LANGUAGE=en;PID=1.2,3;PREF=1;TYPE=work;ALTID=1;X-any=bar:text
GROUP2.NOTE;VALUE=text;LANGUAGE=fr;PID=1.2,3;PREF=2;TYPE=home;ALTID=2;IANA=foo:text
GROUP1.ORG;VALUE=text;SORT-AS=foo,bar;LANGUAGE=en;PID=1.2,3;PREF=1;ALTID=1;TYPE=work;X-any=bar:component;component
GROUP2.ORG;VALUE=text;SORT-AS=bar,foo;LANGUAGE=fr;PID=1.2,3;PREF=2;ALTID=2;TYPE=home;IANA=foo:component;component
GROUP1.PHOTO;VALUE=uri;ALTID=1;TYPE=work;MEDIATYPE=type/sub;PREF=1;PID=1.2,3;X-any=bar:uri:uri
GROUP2.PHOTO;VALUE=uri;ALTID=2;TYPE=home;MEDIATYPE=type/sub;PREF=2;PID=1.2,3;IANA=foo:uri:uri
PRODID;VALUE=text;X-any=bar:text
GROUP1.RELATED;VALUE=uri;MEDIATYPE=type/sub;PID=1.2,3;PREF=1;ALTID=1;TYPE=contact;X-any=bar:uri:uri
GROUP2.RELATED;VALUE=text;LANGUAGE=en;PID=1.2,3;PREF=2;ALTID=2;TYPE=acquaintance;IANA=foo:text
REV;VALUE=timestamp;X-any=bar:19951031T222710Z
GROUP1.ROLE;VALUE=text;LANGUAGE=en;PID=1.2,3;PREF=1;TYPE=work;ALTID=1;X-any=bar:text
GROUP2.ROLE;VALUE=text;LANGUAGE=fr;PID=1.2,3;PREF=2;TYPE=home;ALTID=2;IANA=foo:text
GROUP1.SOUND;VALUE=uri;LANGUAGE=en;PID=1.2,3;PREF=1;TYPE=work;MEDIATYPE=type/sub;ALTID=1;X-any=bar:uri:uri
GROUP2.SOUND;VALUE=uri;LANGUAGE=fr;PID=1.2,3;PREF=2;TYPE=home;MEDIATYPE=type/sub;ALTID=2;IANA=foo:uri:uri
GROUP1.SOURCE;VALUE=uri;PID=1.2,3;PREF=1;ALTID=1;MEDIATYPE=type/sub;x-any=bar:uri:uri
GROUP2.SOURCE;VALUE=uri;PID=1.2,3;PREF=2;ALTID=2;MEDIATYPE=type/sub;iana=foo:uri:uri
GROUP1.TEL;VALUE=text;TYPE=text;PID=1.2,3;PREF=1;ALTID=1;x-any=bar:text
GROUP2.TEL;VALUE=uri;MEDIATYPE=type/sub;TYPE=voice;PID=1.2,3;PREF=2;ALTID=2;iana=foo:uri:uri
GROUP1.TITLE;VALUE=text;LANGUAGE=en;PID=1.2,3;PREF=1;ALTID=1;TYPE=work;X-any=bar:text
GROUP2.TITLE;VALUE=text;LANGUAGE=fr;PID=1.2,3;PREF=2;ALTID=2;TYPE=home;iana=foo:text
GROUP1.TZ;VALUE=text;ALTID=1;PID=1.2,3;PREF=1;TYPE=work;MEDIATYPE=type/sub;X-any=bar:text
GROUP2.TZ;VALUE=uri;ALTID=2;PID=1.2,3;PREF=2;TYPE=home;MEDIATYPE=type/sub;iana=foo:uri:uri
GROUP2.TZ;VALUE=utc-offset;ALTID=1;PID=1.2,3;PREF=1;TYPE=work;MEDIATYPE=type/sub;X-any=bar:-0500
UID;VALUE=uri;x-any=bar:uri:uri
GROUP1.URL;VALUE=uri;PID=1.2,3;PREF=1;TYPE=work;MEDIATYPE=type/sub;ALTID=1;x-any=bar:uri:uri
GROUP2.URL;VALUE=uri;PID=1.2,3;PREF=2;TYPE=home;MEDIATYPE=type/sub;ALTID=2;iana=foo:uri:uri
GROUP1.XML;VALUE=text;ALTID=1:text
GROUP2.XML;VALUE=text;ALTID=2:text
GROUP1.X-ANY;LANGUAGE=en;VALUE=text;PREF=1;ALTID=1;PID=1.2,3;TYPE=work;MEDIATYPE=type/sub;CALSCALE=gregorian;SORT-AS=foo,bar;GEO="uri:uri";TZ="uri:uri":text
GROUP2.X-ANY;LANGUAGE=fr;VALUE=x-value;PREF=2;ALTID=2;PID=1.2,3;TYPE=home;MEDIATYPE=type/sub;CALSCALE=gregorian;SORT-AS=bar,foo;GEO="uri:uri";TZ=param-value:x-value
END:VCARD"#
        .as_bytes();
    let vcard = VcardParser::new(vcard);
    for card in vcard {
        let card = card.unwrap();
        VCard::try_from(card).unwrap();
        // println!("{vcard}")
    }
}
