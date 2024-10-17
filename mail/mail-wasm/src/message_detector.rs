use proton_mail_html_transformer::{message_detector::SplitDoc, Transformer};
use wasm_bindgen::prelude::*;

// To build the wasm version
// wasm-pack build -d ./web/pkg --target web --features="web"

#[wasm_bindgen(getter_with_clone)]
pub struct LocateBlockquoteResult(pub String, pub String);
#[wasm_bindgen]
#[must_use]
pub fn locate_blockquote(input: &str) -> LocateBlockquoteResult {
    let SplitDoc {
        message,
        blockquote,
    } = Transformer::new(input).extract_blockquote();

    let message = message.to_string();
    let blockquote = match blockquote {
        Some(bq) => bq.to_string(),
        None => String::new(),
    };

    LocateBlockquoteResult(message.to_string(), blockquote)
}
