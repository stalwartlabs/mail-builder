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

use std::collections::HashMap;

use crate::encoders::encode::rfc2047_encode;

use super::Header;

/// MIME Content-Type or Content-Disposition header
pub struct ContentType<'x> {
    pub c_type: &'x str,
    pub attributes: HashMap<&'x str, &'x str>,
}

impl<'x> ContentType<'x> {
    /// Create a new Content-Type or Content-Disposition header
    pub fn new(c_type: &'x str) -> Self {
        Self {
            c_type,
            attributes: HashMap::new(),
        }
    }

    /// Set a Content-Type / Content-Disposition attribute
    pub fn attribute(mut self, key: &'x str, value: &'x str) -> Self {
        self.attributes.insert(key, value);
        self
    }
}

impl<'x> Header for ContentType<'x> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        mut bytes_written: usize,
    ) -> std::io::Result<usize> {
        output.write_all(self.c_type.as_bytes())?;
        bytes_written += self.c_type.len();
        if !self.attributes.is_empty() {
            output.write_all(b"; ")?;
            bytes_written += 2;
            for (pos, (key, value)) in self.attributes.iter().enumerate() {
                if bytes_written + key.len() + value.len() + 3 >= 76 {
                    output.write_all(b"\r\n\t")?;
                    bytes_written = 1;
                }

                output.write_all(key.as_bytes())?;
                output.write_all(b"=")?;
                bytes_written += rfc2047_encode(value, &mut output)? + key.len() + 1;
                if pos < self.attributes.len() - 1 {
                    output.write_all(b"; ")?;
                    bytes_written += 2;
                }
            }
        }
        output.write_all(b"\r\n")?;
        Ok(0)
    }
}
