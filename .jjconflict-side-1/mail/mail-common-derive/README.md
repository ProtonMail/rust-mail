# mail-common-derive

This crate provides derive macros specific to mail functionality in the Proton Mail Rust codebase.

## ScrollerEq Derive

The `ScrollerEq` derive macro generates a custom equality implementation that allows you to skip certain fields from comparison. This is particularly useful for conversation objects where certain metadata fields (like timestamps, counts, or UI state) shouldn't affect equality for scrolling/comparison purposes.

### Usage

Add the derive macro to your struct and use `#[scroller_eq(skip)]` to exclude fields from comparison:

```rust
use mail_common_derive::ScrollerEq;
use crate::traits::ScrollerEq as _; // Import the trait

#[derive(ScrollerEq)]
struct Conversation {
    id: u64,
    subject: String,
    sender: String,

    // These fields will be ignored in scroller_eq comparison
    #[scroller_eq(skip)]
    unread_count: u32,
    #[scroller_eq(skip)]
    last_updated: u64,
    #[scroller_eq(skip)]
    display_order: u64,
}
```

### Generated Implementation

The macro generates an implementation of the `ScrollerEq` trait:

```rust
impl ScrollerEq for Conversation {
    fn scroller_eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.subject == other.subject
            && self.sender == other.sender
        // unread_count, last_updated, and display_order are skipped
    }
}
```

### Example

```rust
let conv1 = Conversation {
    id: 1,
    subject: "Hello".to_string(),
    sender: "alice@example.com".to_string(),
    unread_count: 5,
    last_updated: 1000,
    display_order: 100,
};

let conv2 = Conversation {
    id: 1,
    subject: "Hello".to_string(),
    sender: "alice@example.com".to_string(),
    unread_count: 10,    // Different (but skipped)
    last_updated: 2000,  // Different (but skipped)
    display_order: 200,  // Different (but skipped)
};

// These are considered equal for scrolling purposes
assert!(conv1.scroller_eq(&conv2));

// But they're different for regular equality
assert_ne!(conv1, conv2);
```

### Requirements

- The struct must have named fields
- Each field type must implement `PartialEq`

### Attributes

- `#[scroller_eq(skip)]`: Skip this field from equality comparison

## Integration

This crate is designed to work with the `proton-mail-common` crate, where the `ScrollerEq` trait is defined in `crate::traits::ScrollerEq`.
