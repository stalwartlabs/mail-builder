#![cfg(all(target_family = "wasm", target_os = "unknown"))]

use mail_builder::MessageBuilder;
use wasm_bindgen_test::wasm_bindgen_test;

// Default Date, Message-ID and boundaries read the clock without panicking on wasm32-unknown-unknown.
#[wasm_bindgen_test]
fn builds_message_with_generated_headers() {
    let message = MessageBuilder::new()
        .from(("John Doe", "john@doe.com"))
        .to("jane@doe.com")
        .subject("Hello")
        .text_body("text")
        .html_body("<p>html</p>")
        .write_to_string()
        .unwrap();

    assert!(message.contains("\r\nDate: "));
    assert!(message.contains("Message-ID: <"));
    assert!(message.contains("multipart/alternative"));
    assert!(!message.contains("Date: Thu, 1 Jan 1970"));
}
