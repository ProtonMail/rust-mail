use super::ScrollerEq;
use proton_mail_common_derive::ScrollerEq;

#[derive(ScrollerEq, PartialEq, Debug)]
struct TestConversation {
    id: u64,
    subject: String,
    sender: String,
    #[scroller_eq(skip)]
    unread_count: u32,
    #[scroller_eq(skip)]
    last_updated: u64,
}

#[test]
fn test_scroller_eq_skips_marked_fields() {
    let conv1 = TestConversation {
        id: 1,
        subject: "Test Subject".to_string(),
        sender: "test@example.com".to_string(),
        unread_count: 5,
        last_updated: 1000,
    };

    let conv2 = TestConversation {
        id: 1,
        subject: "Test Subject".to_string(),
        sender: "test@example.com".to_string(),
        unread_count: 10,   // Different unread count (skipped)
        last_updated: 2000, // Different timestamp (skipped)
    };

    let conv3 = TestConversation {
        id: 2, // Different ID (not skipped)
        subject: "Test Subject".to_string(),
        sender: "test@example.com".to_string(),
        unread_count: 10,
        last_updated: 2000,
    };

    // These should be equal via scroller_eq despite different skipped fields
    assert!(conv1.scroller_eq(&conv2));

    // These should not be equal because ID is different (not skipped)
    assert!(!conv1.scroller_eq(&conv3));

    // Regular PartialEq should still work differently
    assert_ne!(conv1, conv2); // Different due to unread_count and last_updated
}

#[test]
fn test_scroller_eq_compares_non_skipped_fields() {
    let conv1 = TestConversation {
        id: 1,
        subject: "Subject 1".to_string(),
        sender: "test@example.com".to_string(),
        unread_count: 5,
        last_updated: 1000,
    };

    let conv2 = TestConversation {
        id: 1,
        subject: "Subject 2".to_string(), // Different subject (not skipped)
        sender: "test@example.com".to_string(),
        unread_count: 10,   // Different unread count (skipped)
        last_updated: 2000, // Different timestamp (skipped)
    };

    // Should not be equal because subject is different and not skipped
    assert!(!conv1.scroller_eq(&conv2));
}

#[derive(Debug, ScrollerEq)]
#[allow(dead_code)]
struct AllFieldsSkipped {
    #[scroller_eq(skip)]
    field1: u32,
    #[scroller_eq(skip)]
    field2: String,
}

#[test]
fn test_scroller_eq_all_fields_skipped() {
    let s1 = AllFieldsSkipped {
        field1: 1,
        field2: "hello".to_string(),
    };
    let s2 = AllFieldsSkipped {
        field1: 999,
        field2: "world".to_string(),
    };

    // Should be equal since all fields are skipped
    assert!(s1.scroller_eq(&s2));
}
