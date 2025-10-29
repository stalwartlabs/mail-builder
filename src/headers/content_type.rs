/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use std::borrow::Cow;

use crate::encoders::encode::rfc2231_encode_parameter;

use super::Header;

/// MIME Content-Type or Content-Disposition header
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ContentType<'x> {
    pub c_type: Cow<'x, str>,
    pub attributes: Vec<(Cow<'x, str>, Cow<'x, str>)>,
}

impl<'x> ContentType<'x> {
    /// Create a new Content-Type or Content-Disposition header
    pub fn new(c_type: impl Into<Cow<'x, str>>) -> Self {
        Self {
            c_type: c_type.into(),
            attributes: Vec::new(),
        }
    }

    /// Set a Content-Type / Content-Disposition attribute
    pub fn attribute(
        mut self,
        key: impl Into<Cow<'x, str>>,
        value: impl Into<Cow<'x, str>>,
    ) -> Self {
        self.attributes.push((key.into(), value.into()));
        self
    }

    /// Returns true when the part is text/*
    pub fn is_text(&self) -> bool {
        self.c_type.starts_with("text/")
    }

    /// Returns true when the part is an attachment
    pub fn is_attachment(&self) -> bool {
        self.c_type == "attachment"
    }
}

impl Header for ContentType<'_> {
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
                let parameter_len = key.len() + value.len() + 3;

                if pos != 0 {
                    if bytes_written + parameter_len >= 75 {
                        output.write_all(b";")?;
                        bytes_written += 1;
                    } else {
                        output.write_all(b"; ")?;
                        bytes_written += 2;
                    }
                }

                if bytes_written + parameter_len >= 76 {
                    output.write_all(b"\r\n\t")?;
                    bytes_written = 1;
                }

                bytes_written = rfc2231_encode_parameter(key, value, &mut output, bytes_written)?;
            }
        }
        output.write_all(b"\r\n")?;
        Ok(0)
    }
}
