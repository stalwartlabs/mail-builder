/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use super::{
    Header,
    fold::{FoldWriter, write_b_words, write_q_words, write_unstructured},
};
use crate::{
    encoders::encode::{EncodingType, get_encoding_type},
    writer::Writer,
};
use std::borrow::Cow;

/// Unstructured text e-mail header.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Text<'x> {
    pub text: Cow<'x, str>,
}

impl<'x> Text<'x> {
    /// Create a new unstructured text header
    pub fn new(text: impl Into<Cow<'x, str>>) -> Self {
        Self { text: text.into() }
    }
}

impl<'x, T> From<T> for Text<'x>
where
    T: Into<Cow<'x, str>>,
{
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl Header for Text<'_> {
    fn write_header(&self, output: &mut impl Writer, column: usize) {
        let mut folder = FoldWriter::new(output, column);

        match get_encoding_type(self.text.as_bytes(), true, false) {
            EncodingType::Base64 => write_b_words(&mut folder, &self.text, b""),
            EncodingType::QuotedPrintable(is_ascii) => {
                write_q_words::<_, false>(&mut folder, &self.text, is_ascii, b"")
            }
            EncodingType::None => write_unstructured(&mut folder, self.text.as_bytes()),
        }

        folder.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mail_parser::MessageParser;

    #[test]
    fn test_utf8_q_encoding_boundaries() {
        let mut buf = b"Subject: ".to_vec();

        let mut input = String::new();

        for _ in 0..20000 {
            input += "x";
        }
        for _ in 0..600 {
            input += "δ";
        }

        input += "x";
        for _ in 0..600 {
            input += "δ";
        }

        let header = Text::new(input.clone());
        header.write_header(&mut buf, "Subject: ".len());

        let output = str::from_utf8(&buf).unwrap();

        for line in output.lines() {
            assert!(
                line.trim().len() <= 78,
                "Line exceeds 78 characters: {}",
                line
            );
        }
        let message = MessageParser::new()
            .parse_headers(output.as_bytes())
            .unwrap();
        assert_eq!(message.subject().unwrap(), input);

        assert!(output.starts_with("Subject: =?utf-8?Q?xxx"));

        assert!(!output.contains("CE?="));
        assert!(!output.contains("=?utf-8?Q?=B4"));
    }

    fn b_encoded_input() -> String {
        let mut input = String::new();

        for _ in 0..600 {
            input += "δ";
        }
        input += "x";
        for _ in 0..600 {
            input += "δ";
        }
        input
    }

    #[test]
    fn test_utf8_b_encoding_boundaries() {
        let mut buf = b"Subject: ".to_vec();

        let input = b_encoded_input();

        let header = Text::new(input.clone());
        header.write_header(&mut buf, "Subject: ".len());

        let output = str::from_utf8(&buf).unwrap();
        for line in output.lines() {
            assert!(
                line.trim().len() <= 78,
                "Line exceeds 78 characters: {}",
                line
            );
        }
        let message = MessageParser::new()
            .parse_headers(output.as_bytes())
            .unwrap();
        assert_eq!(message.subject().unwrap(), input);

        assert!(output.starts_with("Subject: =?utf-8?B?zrTOtM60zrTOtM60"));

        assert!(!output.contains("zg==?="));
        assert!(!output.contains("?B?tM60zr"));

        assert!(output.ends_with("\r\n"));
    }

    #[test]
    fn test_utf8_b_encoding_large_bytes_written() {
        let mut buf = Vec::new();

        let input = b_encoded_input();

        let header = Text::new(input);

        let bytes_written = 500;
        header.write_header(&mut buf, bytes_written);

        let output = str::from_utf8(&buf).unwrap();

        for line in output.lines() {
            assert!(
                line.trim().len() <= 78,
                "Line exceeds 78 characters: {}",
                line
            );
        }

        assert!(
            output.starts_with("\r\n =?utf-8?B?zrTOtM60zrTOtM60"),
            "{output:?}"
        );
    }
}
