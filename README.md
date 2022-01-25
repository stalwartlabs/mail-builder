# mail-builder

[![crates.io](https://img.shields.io/crates/v/mail-builder)](https://crates.io/crates/mail-builder)
[![build](https://github.com/stalwartlabs/mail-builder/actions/workflows/rust.yml/badge.svg)](https://github.com/stalwartlabs/mail-builder/actions/workflows/rust.yml)
[![docs.rs](https://img.shields.io/docsrs/mail-builder)](https://docs.rs/mail-builder)
[![crates.io](https://img.shields.io/crates/l/mail-builder)](http://www.apache.org/licenses/LICENSE-2.0)

_mail-builder_ is a flexible **e-mail builder library** written in Rust that generates RFC5322 compliant e-mail messages. 
The library has full MIME support and automatically selects the most optimal encoding for each message body part.

Building e-mail messages is straightforward:

```rust
    // Build a simple text message with a single attachment
    let mut message = MessageBuilder::new();
    message.from(("John Doe", "john@doe.com"));
    message.to("jane@doe.com");
    message.subject("Hello, world!");
    message.text_body("Message contents go here.");
    message.binary_attachment("image/png", "image.png", &[1, 2, 3, 4]);

    // Write message to memory
    let mut output = Vec::new();
    message.write_to(&mut output).unwrap();
```

More complex messages with grouped addresses, inline parts and 
multipart/alternative sections can also be easily built:

```rust
    // Build a multipart message with text and HTML bodies,
    // inline parts and attachments.
    let mut message = MessageBuilder::new();
    message.from(("John Doe", "john@doe.com"));

    // To recipients
    message.to(vec![
        ("Antoine de Saint-Exupéry", "antoine@exupery.com"),
        ("안녕하세요 세계", "test@test.com"),
        ("Xin chào", "addr@addr.com"),
    ]);

    // BCC recipients using grouped addresses
    message.bcc(vec![
        (
            "My Group",
            vec![
                ("ASCII name", "addr1@addr7.com"),
                ("ハロー・ワールド", "addr2@addr6.com"),
                ("áéíóú", "addr3@addr5.com"),
                ("Γειά σου Κόσμε", "addr4@addr4.com"),
            ],
        ),
        (
            "Another Group",
            vec![
                ("שלום עולם", "addr5@addr3.com"),
                ("ñandú come ñoquis", "addr6@addr2.com"),
                ("Recipient", "addr7@addr1.com"),
            ],
        ),
    ]);

    // Set RFC and custom headers
    message.subject("Testing multipart messages");
    message.in_reply_to(vec!["message-id-1", "message-id-2"]);
    message.header("List-Archive", URL::new("http://example.com/archive"));

    // Set HTML and plain text bodies
    message.text_body("This is the text body!\n");
    message.html_body("<p>HTML body with <img src=\"cid:my-image\"/>!</p>");

    // Include an embedded image as an inline part
    message.binary_inline("image/png", "cid:my-image", &[0, 1, 2, 3, 4, 5]);

    // Add a text and a binary attachment
    message.text_attachment("text/plain", "my fíle.txt", "Attachment contents go here.");
    message.binary_attachment(
        "text/plain",
        "ハロー・ワールド",
        b"Binary contents go here.",
    );

    // Write the message to a file
    message
        .write_to(File::create("message.eml").unwrap())
        .unwrap();
```

Nested MIME body structures can be created using the `body` method:

```rust
    // Build a nested multipart message
    let mut message = MessageBuilder::new();

    message.from(Address::new_address("John Doe".into(), "john@doe.com"));
    message.to(Address::new_address("Jane Doe".into(), "jane@doe.com"));
    message.subject("Nested multipart message");

    // Define the nested MIME body structure
    message.body(MimePart::new_multipart(
        "multipart/mixed",
        vec![
            MimePart::new_text("Part A contents go here...").inline(),
            MimePart::new_multipart(
                "multipart/mixed",
                vec![
                    MimePart::new_multipart(
                        "multipart/alternative",
                        vec![
                            MimePart::new_multipart(
                                "multipart/mixed",
                                vec![
                                    MimePart::new_text("Part B contents go here...").inline(),
                                    MimePart::new_binary(
                                        "image/jpeg",
                                        "Part C contents go here...".as_bytes(),
                                    )
                                    .inline(),
                                    MimePart::new_text("Part D contents go here...").inline(),
                                ],
                            ),
                            MimePart::new_multipart(
                                "multipart/related",
                                vec![
                                    MimePart::new_html("Part E contents go here...").inline(),
                                    MimePart::new_binary(
                                        "image/jpeg",
                                        "Part F contents go here...".as_bytes(),
                                    ),
                                ],
                            ),
                        ],
                    ),
                    MimePart::new_binary("image/jpeg", "Part G contents go here...".as_bytes())
                        .attachment("image_G.jpg"),
                    MimePart::new_binary(
                        "application/x-excel",
                        "Part H contents go here...".as_bytes(),
                    ),
                    MimePart::new_binary(
                        "x-message/rfc822",
                        "Part J contents go here...".as_bytes(),
                    ),
                ],
            ),
            MimePart::new_text("Part K contents go here...").inline(),
        ],
    ));

    // Write the message to a file
    message
        .write_to(File::create("nested-message.eml").unwrap())
        .unwrap();
```

Please note that this library does not support parsing e-mail messages as this functionality is provided separately by the [`mail-parser`](https://crates.io/crates/mail-parser) crate.


## Testing

To run the testsuite:

```bash
 $ cargo test --all-features
```

or, to run the testsuite with MIRI:

```bash
 $ cargo +nightly miri test --all-features
```

## License

Licensed under either of

 * Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## Copyright

Copyright (C) 2020-2022, Stalwart Labs, Minter Ltd.

See [COPYING] for the license.

[COPYING]: https://github.com/stalwartlabs/mail-builder/blob/main/COPYING
