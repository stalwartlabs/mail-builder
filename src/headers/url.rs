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

/// URL header, used mostly on List-* headers
pub struct URL<'x> {
    pub url: Vec<&'x str>,
}

impl<'x> URL<'x> {
    /// Create a new URL header
    pub fn new(url: &'x str) -> Self {
        Self { url: vec![url] }
    }

    /// Create a new multi-value URL header
    pub fn new_list(urls: &[&'x str]) -> Self {
        Self { url: urls.to_vec() }
    }
}

impl<'x> From<&'x str> for URL<'x> {
    fn from(value: &'x str) -> Self {
        Self::new(value)
    }
}

impl<'x> From<&[&'x str]> for URL<'x> {
    fn from(value: &[&'x str]) -> Self {
        Self::new_list(value)
    }
}

impl<'x> From<Vec<&'x str>> for URL<'x> {
    fn from(value: Vec<&'x str>) -> Self {
        URL { url: value }
    }
}

impl<'x> Header for URL<'x> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        mut bytes_written: usize,
    ) -> std::io::Result<usize> {
        for (pos, url) in self.url.iter().enumerate() {
            output.write_all(b"<")?;
            output.write_all(url.as_bytes())?;
            output.write_all(b">")?;
            bytes_written += url.len() + 2;
            if bytes_written >= 76 && pos < self.url.len() - 1 {
                output.write_all(b"\r\n\t")?;
                bytes_written = 0;
            }
        }
        if bytes_written > 0 {
            output.write_all(b"\r\n")?;
        }
        Ok(0)
    }
}
