use proton_mail_common::actions::messages::UnsubscribeNewsletter;
use proton_mail_common::datatypes::ParsedHeaders;
use serde_json::Value;
use std::collections::HashMap;

fn add(map: &mut ParsedHeaders, v: &str) {
    map.headers
        .insert("List-Unsubscribe".to_owned(), Value::from(String::from(v)));
}

#[test]
fn correct_header_parsing() {
    let id = 0.into();
    let headers = &mut ParsedHeaders {
        headers: HashMap::new(),
    };

    {
        add(headers, "<https://foo.bar/subscribe>");
        UnsubscribeNewsletter::new(headers, id).unwrap();
    }

    {
        add(
            headers,
            "<https://foo.bar/subscribe>, <mailto:unsubscribe@bar.com/subscribe>",
        );
        let u = UnsubscribeNewsletter::new(headers, id).unwrap();
        assert!(u.request.is_some());
        assert!(u.mail.is_some());

        add(
            headers,
            "<https://foo.bar/subscribe>,<mailto:unsubscribe@bar.com/subscribe>",
        );

        let u2 = UnsubscribeNewsletter::new(headers, id).unwrap();
        assert_eq!(u, u2);
    }

    {
        add(
            headers,
            "<mailto:unsubscribe@bar.com/subscribe?subject=foo&?body=bar>",
        );

        assert!(
            UnsubscribeNewsletter::new(headers, id).is_none(),
            "This should fail when unsubscribe via mail is implemented"
        );
    }
}
