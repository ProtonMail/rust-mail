#![no_main]

#[macro_use]
extern crate libfuzzer_sys;

fuzz_target!(|data: &str| {
    _ = proton_mail_html_transformer::Transformer::new(data);
});
