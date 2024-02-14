use wasm_bindgen::prelude::*;

// To build the wasm version
// wasm-pack build -d ./web/pkg --target web --features="web"

#[wasm_bindgen(getter_with_clone)]
pub struct LocateBlockquoteResult(pub String, pub String);
#[wasm_bindgen]
pub fn locate_blockquote(input: &str) -> LocateBlockquoteResult {
    let (before, after) = proton_mail_message_detector::locate_blockquote(input);
    LocateBlockquoteResult(before, after)
}
