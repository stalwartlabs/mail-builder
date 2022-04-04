/*
 * Copyright Stalwart Labs Ltd. See the COPYING
 * file at the top-level directory of this distribution.
 *
 * Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
 * https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
 * <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
 * option. This file may not be copied, modified, or distributed
 * except according to those terms.
 */

use std::fs::File;

use mail_builder::{headers::address::Address, mime::MimePart, MessageBuilder};

fn main() {
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
}
