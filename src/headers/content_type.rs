/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use super::Header;
use crate::encoders::encode::rfc2047_encode;
use std::borrow::Cow;

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

#[cfg(test)]
mod tests {
    use super::*;

    fn build(content_type: ContentType<'_>) -> String {
        let mut output = Vec::new();
        content_type.write_header(&mut output, 14).unwrap();
        String::from_utf8(output).unwrap()
    }

    #[test]
    fn encoded_parameter_value_stays_quoted() {
        let header =
            build(ContentType::new("attachment").attribute("filename", "Jahresabschluß, 2024.pdf"));
        assert!(header.contains("filename=\"=?"), "{header:?}");
        assert!(header.contains("?=\""), "{header:?}");
    }

    #[test]
    fn plain_parameter_value_is_quoted_and_escaped() {
        let header =
            build(ContentType::new("attachment").attribute("filename", "report \"final\".pdf"));
        assert!(header.contains(r#""report \"final\".pdf""#), "{header:?}");
    }
}
