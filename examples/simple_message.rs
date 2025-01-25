/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use mail_builder::MessageBuilder;

fn main() {
    // Build a simple text message with a single attachment
    let eml = MessageBuilder::new()
        .from(("John Doe", "john@doe.com"))
        .to("jane@doe.com")
        .subject("Hello, world!")
        .text_body("Message contents go here.")
        .attachment("image/png", "image.png", [1, 2, 3, 4].as_ref())
        .write_to_string()
        .unwrap();

    // Print raw message
    println!("{}", eml);
}
