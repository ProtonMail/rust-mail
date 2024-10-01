use velcro::hash_set;

use crate::properties::categories::{validate_categories, Category};
use crate::test::{make_property, property_reject_parameters};
use crate::values::text_list::TextList;
use crate::ParameterType;

#[test]
fn categories_struct() {
    let categories = Category::new_validated(&["A".to_owned(), "b".to_owned()]).unwrap();
    assert_eq!(
        categories.value,
        TextList::new_validated(&["A".to_owned(), "b".to_owned()]).unwrap()
    );
}

#[test]
fn categories_property() {
    validate_categories(&make_property("CATEGORIES", Some("text-list"), None)).unwrap();
    validate_categories(&make_property(
        "CATEGORIES",
        Some("text-list"),
        Some(vec![
            ("VALUE", vec!["text"]),
            ("PID", vec!["1.2", "3.4"]),
            ("PREF", vec!["1"]),
            ("TYPE", vec!["type/subtype"]),
            ("ALTID", vec!["param-value"]),
            ("any", vec!["foo", "bar"]),
        ]),
    ))
    .unwrap();
    property_reject_parameters(
        validate_categories,
        "CATEGORIES",
        "text-list",
        hash_set! {ParameterType::CalScale, ParameterType::Geo, ParameterType::Label, ParameterType::Language, ParameterType::MediaType, ParameterType::SortAs, ParameterType::TZ},
    );
}
