/*
 * Copyright Stalwart Labs, Minter Ltd. See the COPYING
 * file at the top-level directory of this distribution.
 *
 * Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
 * https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
 * <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
 * option. This file may not be copied, modified, or distributed
 * except according to those terms.
 */

use super::Header;

/// Raw e-mail header.
/// Raw headers are not encoded, only line-wrapped.
pub struct Raw<'x> {
    pub raw: &'x str,
}

impl<'x> Raw<'x> {
    /// Create a new raw header
    pub fn new(raw: &'x str) -> Self {
        Self { raw }
    }
}

impl<'x> From<&'x str> for Raw<'x> {
    fn from(value: &'x str) -> Self {
        Self::new(value)
    }
}

impl<'x> Header for Raw<'x> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        mut bytes_written: usize,
    ) -> std::io::Result<usize> {
        for (pos, &ch) in self.raw.as_bytes().iter().enumerate() {
            if bytes_written >= 76 && ch.is_ascii_whitespace() && pos < self.raw.len() - 1 {
                output.write_all(b"\r\n\t")?;
                bytes_written = 1;
            }
            output.write_all(&[ch])?;
            bytes_written += 1;
        }
        output.write_all(b"\r\n")?;
        Ok(0)
    }
}
