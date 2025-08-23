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

pub struct Base64Encoder(bool);
pub struct QuotedPrintableEncoder(bool);

impl Base64Encoder {
    pub fn new() -> Self {
        Self(true)
    }

    pub fn indent(mut self) -> Self {
        self.0 = false;
        self
    }

    #[inline(always)]
    pub fn encode(&self, input: &[u8]) -> io::Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(4 * (input.len() / 3));
        base64_encode_mime(input, &mut buf, self.0)?;
        Ok(buf)
    }

    pub fn encode_to_writer(&self, input: &[u8], output: &mut impl Write) -> io::Result<usize> {
        base64_encode_mime(input, output, self.0)
    }
}

impl QuotedPrintableEncoder {
    pub fn new() -> Self {
        Self(false)
    }

    pub fn indent(mut self) -> Self {
        self.0 = true;
        self
    }

    #[inline(always)]
    pub fn encode(&self, input: &[u8]) -> io::Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(input.len() * 2);
        quoted_printable_encode(input, &mut buf, self.0)?;
        Ok(buf)
    }

    pub fn encode_to_writer(&self, input: &[u8], output: &mut impl Write) -> io::Result<usize> {
        quoted_printable_encode(input, output, self.0)
    }
}

impl Default for Base64Encoder {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for QuotedPrintableEncoder {
    fn default() -> Self {
        Self::new()
    }
}
