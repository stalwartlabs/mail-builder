/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use crate::encoders::{base64::base64_encode_mime, quoted_printable::quoted_printable_encode};
use std::io::{self, Write};

pub mod base64;
pub mod encode;
pub mod quoted_printable;

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Base64Encoder {
    wrap_lines: bool,
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QuotedPrintableEncoder {
    preserve_line_breaks: bool,
}

impl Base64Encoder {
    #[inline(always)]
    pub fn new() -> Self {
        Self { wrap_lines: false }
    }

    #[inline(always)]
    pub fn wrap_lines(mut self) -> Self {
        self.wrap_lines = true;
        self
    }

    #[inline(always)]
    pub fn encode(&self, input: &[u8]) -> io::Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(4 * (input.len() / 3));
        base64_encode_mime(input, &mut buf, !self.wrap_lines)?;
        Ok(buf)
    }

    #[inline(always)]
    pub fn encode_to_writer(&self, input: &[u8], output: &mut impl Write) -> io::Result<usize> {
        base64_encode_mime(input, output, !self.wrap_lines)
    }
}

impl QuotedPrintableEncoder {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            preserve_line_breaks: false,
        }
    }

    #[inline(always)]
    pub fn preserve_line_breaks(mut self) -> Self {
        self.preserve_line_breaks = true;
        self
    }

    #[inline(always)]
    pub fn encode(&self, input: &[u8]) -> io::Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(input.len() * 2);
        quoted_printable_encode(input, &mut buf, self.preserve_line_breaks)?;
        Ok(buf)
    }

    #[inline(always)]
    pub fn encode_to_writer(&self, input: &[u8], output: &mut impl Write) -> io::Result<usize> {
        quoted_printable_encode(input, output, self.preserve_line_breaks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encoder_does_not_wrap_by_default() {
        let encoded = Base64Encoder::new().encode(&[b'x'; 300]).unwrap();
        assert!(!encoded.contains(&b'\n'), "unexpected line break");
    }

    #[test]
    fn base64_encoder_wraps_lines_when_requested() {
        let encoded = Base64Encoder::new()
            .wrap_lines()
            .encode(&[b'x'; 300])
            .unwrap();
        assert!(encoded.windows(2).any(|w| w == b"\r\n"), "no line break");
        for line in encoded.split(|&ch| ch == b'\n') {
            assert!(line.len() <= 77, "line too long: {}", line.len());
        }
    }

    #[test]
    fn quoted_printable_encoder_escapes_line_breaks_by_default() {
        let encoded = QuotedPrintableEncoder::new().encode(b"a\r\nb").unwrap();
        assert_eq!(encoded, b"a=0D=0Ab");
    }

    #[test]
    fn quoted_printable_encoder_preserves_line_breaks_when_requested() {
        let encoded = QuotedPrintableEncoder::new()
            .preserve_line_breaks()
            .encode(b"a\r\nb")
            .unwrap();
        assert_eq!(encoded, b"a\r\nb");
    }
}
