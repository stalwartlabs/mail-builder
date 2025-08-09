/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use std::borrow::Cow;

use crate::encoders::{
    base64::base64_encode_mime,
    encode::{get_encoding_type, EncodingType},
    quoted_printable::quoted_printable_encode_byte,
};

use super::Header;

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
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        mut bytes_written: usize,
    ) -> std::io::Result<usize> {
        match get_encoding_type(self.text.as_bytes(), true, false) {
            EncodingType::Base64 => {
                for (pos, chunk) in self.text.as_bytes().chunks(76 - bytes_written).enumerate() {
                    if pos > 0 {
                        output.write_all(b"\t")?;
                    }
                    output.write_all(b"=?utf-8?B?")?;
                    base64_encode_mime(chunk, &mut output, true)?;
                    output.write_all(b"?=\r\n")?;
                }
            }
            EncodingType::QuotedPrintable(is_ascii) => {
                let prefix = if is_ascii {
                    b"=?us-ascii?Q?".as_ref()
                } else {
                    b"=?utf-8?Q?".as_ref()
                };

                output.write_all(prefix)?;
                bytes_written += prefix.len();

                for (pos, &ch) in self.text.as_bytes().iter().enumerate() {
                    // (ch as i8) >= 0x40 is an inlined
                    // check for UTF-8 char boundary without array access
                    // taken from private u8.is_char_boundary() implementation:
                    // https://github.com/rust-lang/rust/blob/8708f3cd1f96d226f6420a58ebdd61aa0bc08b0a/library/core/src/str/mod.rs#L360-L383
                    if bytes_written >= 76 && (pos == 0 || (ch as i8) >= -0x40) {
                        output.write_all(b"?=\r\n\t")?;
                        output.write_all(prefix)?;
                        bytes_written = 1 + prefix.len();
                    }

                    bytes_written += quoted_printable_encode_byte(ch, &mut output)?;
                }
                output.write_all(b"?=\r\n")?;
            }
            EncodingType::None => {
                for (pos, &ch) in self.text.as_bytes().iter().enumerate() {
                    if bytes_written >= 76 && ch.is_ascii_whitespace() && pos < self.text.len() - 1
                    {
                        output.write_all(b"\r\n\t")?;
                        bytes_written = 1;
                    }
                    output.write_all(&[ch])?;
                    bytes_written += 1;
                }
                output.write_all(b"\r\n")?;
            }
        }
        Ok(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    /// Tests that UTF-8 encoded words are split only at character boundaries.
    ///
    /// According to RFC 2047
    /// "The 'encoded-text' in each 'encoded-word' must be well-formed according to the encoding specified"
    /// so it is not allowed to split a single UTF-8 character between two encoded-words.
    #[test]
    fn test_utf8_encoding_boundaries() {
        let mut buf = Cursor::new(Vec::new());

        let mut input = String::new();

        // Insert a lot of ASCII characters so the header
        // is encoded as "Q" encoded-words
        // rather than "B" encoded-words.
        for _ in 0..20000 {
            input += "x";
        }
        for _ in 0..600 {
            // δ is encoded as "=CE=B4".
            // It should never be split into "...=CE?="
            // and "=?utf-8?Q?=B4...".
            input += "δ";
        }

        // Shift by 1 byte and insert more non-ASCII.
        input += "x";
        for _ in 0..600 {
            input += "δ";
        }

        let header = Text::new(input);
        header.write_header(&mut buf, 0).unwrap();

        let output = str::from_utf8(buf.get_ref()).unwrap();

        // Test that "Q" encoding is used.
        assert!(output.starts_with("=?utf-8?Q?xxx"));

        assert!(!output.contains("CE?="));
        assert!(!output.contains("=?utf-8?Q?=B4"));
    }
}
