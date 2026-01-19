`engine.export()` retunrs an iterator of [`ExportEvent`]

 - `ExportEvent::Load` shall be handled like the other `LoadEvent`s.
 - `ExportEvent::Entry` contains an export item which can be in turn imported into another engine. 

Use cases:
 - Creating an occasional dump to bootstrap new/empty clients.
 - Compact and filter the index by exporting an old engine and importing only current entries into aone.
Entries from indices are sorted and provided that the indices themself provide sorted export,
the final result will also be sorted. The engine export shall merge subsequent entry attr values
from different indices into one so ideally only one Entry per entry-attribute shall be produced,
so that mixed attribute value indices can be preserved correctly over export/import round trip.

# The `Entry` structure and export/import protocol

There is no protocol built into the engine. Just the API to export/import and the exported types.

The [`Entry`] serde implementation defines the export/import protocol semantics. The application may chose their own particular encoding scheme to exchange exports. Here we give examples of JSON, but CBOR would likely be superior in terms of payload size and performance.

The `Entry` is a map/object with these fields:
  - identifier: a `string` representing the entry ID
  - attributes: a map/object with attribute names as keys (`string`) each containing an array of `EntryValue`s

The `EntryValue` maps known values to the established variants:
  - null (`null`) => `Empty`
  - bool (`true`/`false`) => `Boolean`
  - int number (`1234`) => `Integer`
  - string (`"tag"`) => `Tag`
  - tokens (`[[123,"hello"]]`) => `Text`

The `tokens` are an array of pair tuples. The first of the pair is the byte-wise position of the token within text (int number), second holds the token string.

Example:

```rust
use proton_foundation_search::entry::*;
use std::sync::Arc;

let entry = Entry::new(
    "entry1",
    [
        ("created".into(), Arc::new(vec![93939393.into()])),
        ("category".into(), Arc::new(vec!["story:fantasy".into()])),
        (
            "mixung".into(),
            Arc::new(vec![
                vec![(0, "dwarf".into()), (6, "fairy".into())].into(),
                "mixtag".into(),
                3.into(),
            ]),
        ),
    ]
    .into(),
);
let json = serde_json::to_string_pretty(&entry).expect("serialize");
insta::assert_snapshot!(json, @r#"
{
  "identifier": "entry1",
  "attributes": {
    "category": [
      "story:fantasy"
    ],
    "created": [
      93939393
    ],
    "mixung": [
      [
        [
          0,
          "dwarf"
        ],
        [
          6,
          "fairy"
        ]
      ],
      "mixtag",
      3
    ]
  }
}
"#);

insta::assert_debug_snapshot!(entry, @r#"
Entry {
    identifier: "entry1",
    attributes: {
        "category": [
            Tag(
                "story:fantasy",
            ),
        ],
        "created": [
            Integer(
                93939393,
            ),
        ],
        "mixung": [
            Text(
                [
                    (
                        0,
                        "dwarf",
                    ),
                    (
                        6,
                        "fairy",
                    ),
                ],
            ),
            Tag(
                "mixtag",
            ),
            Integer(
                3,
            ),
        ],
    },
}
"#);
```

# Extensibility

In future, we may introduce additional value types to make them available for indexing. For instance geo-coding coordinates or generic float number. 

When adding a new value type, its interpretation must be unambiguously distinguished from the previous existing types. 

So for instance a float number, which may be whole (no decimals), may be serialized in some schemes as integer, this would break compatibility as the decoder would deserialized it as Integer. If this was the case, It would have to be wrapped in a new type with a serialization tag to carry over the semantics.

On the other hand a geo-coding triplet tuple would not conflict with existing value types and could be added as is, being represented by an array of three floats in the serialized form.

Considering the rules of the game, extending export/import features is quite trivial. Let's look at future-proofing import as well...

# Future-proofing the import

Clients on different platforms at different times will have varying versions of clients installed. These clients will inevitably diverge in the set of recognized engine index values.

The application could/shoul future proof the import when deserializing entries by accommodating
unknown/unrecognized values, decide what to do with them and then import only the known values
into the engine. This would imply some information loss, but the older engine could be populated from
a newer one's export then.

Thought: The engine could also do this - preserve the unrecognized value so it can be exported again without a loss of information... Storing something "undefined" in Rust is problematic. We could for instance use the serde_json `Value` internally, but that would impose constraints on the application, regarding serialization scheme. So let's leave this task to the application for now. Should the app find it necessary, it can store the unsupported values and later in case of an export, enrich the exported entries with the missing pieces again.

```rust
use proton_foundation_search::engine::Engine;
use proton_foundation_search::entry::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

#[derive(Debug, Serialize, Deserialize)]
struct FutureProofEntry {
    identifier: Box<str>,
    attributes: BTreeMap<Box<str>, Vec<FutureProofValue>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(untagged)]
enum FutureProofValue {
    Known(EntryValue),
    Future(serde_json::Value),
}

impl From<FutureProofEntry> for Entry {
    fn from(value: FutureProofEntry) -> Self {
        Entry::new(
            value.identifier,
            value
                .attributes
                .into_iter()
                .map(|(k, v)| (k, Arc::new(v.into_iter().map(Into::into).collect())))
                .collect(),
        )
    }
}

impl From<FutureProofValue> for EntryValue {
    fn from(value: FutureProofValue) -> Self {
        match value {
            FutureProofValue::Known(value) => value,
            FutureProofValue::Future(value) => {
                tracing::warn!("Ignoring unknown value on import {value:#?}");
                EntryValue::Empty
            }
        }
    }
}

// let's imagine a future version of the app will add a new type of a value
// that the current engine doesn't know about. It could be a geo coordinate for instance:
// the `[123.123, 321.321, 111.4]` in the "futured" attribute
let json = r#"
{
"identifier": "entry1",
"attributes": {
    "alright": [
    "mixtag",
    3
    ],
    "futured": [
        [123.123, 321.321, 111.4]
    ]
}
}
"#;

let entry: FutureProofEntry = serde_json::from_str(json).expect("deserialied");

// note that the unknown value is deserialized as a json value
insta::assert_debug_snapshot!(entry, @r#"
FutureProofEntry {
    identifier: "entry1",
    attributes: {
        "alright": [
            Known(
                Tag(
                    "mixtag",
                ),
            ),
            Known(
                Integer(
                    3,
                ),
            ),
        ],
        "futured": [
            Future(
                Array [
                    Number(123.123),
                    Number(321.321),
                    Number(111.4),
                ],
            ),
        ],
    },
}
"#);

let entry_for_import: Entry = entry.into();

// note that the unknown value is replaced with `Empty`
insta::assert_debug_snapshot!(entry_for_import, @r#"
Entry {
    identifier: "entry1",
    attributes: {
        "alright": [
            Tag(
                "mixtag",
            ),
            Integer(
                3,
            ),
        ],
        "futured": [
            Empty,
        ],
    },
}
"#);

// The `entry_for_import` can now be imported into the engine:
let engine = Engine::builder().build();
let mut write = engine.write().expect("writer");
write.import(entry_for_import);
for _event in write.commit() {
    //todo
}
```
