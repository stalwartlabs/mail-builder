use crate::encoders::{
    base64::base64_encode,
    encode::{get_encoding_type, EncodingType},
    quoted_printable::quoted_printable_encode,
};

use super::Header;

pub struct Text<'x> {
    pub text: &'x str,
}

impl<'x> Text<'x> {
    pub fn new(text: &'x str) -> Self {
        Self { text }
    }
}

impl<'x> Header for Text<'x> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        mut bytes_written: usize,
    ) -> std::io::Result<usize> {
        match get_encoding_type(self.text, true) {
            EncodingType::Base64 => {
                for (pos, chunk) in self.text.as_bytes().chunks(76 - bytes_written).enumerate() {
                    if pos > 0 {
                        output.write_all(b"\t")?;
                    }
                    output.write_all(b"=?utf-8?B?")?;
                    base64_encode(chunk, &mut output, true)?;
                    output.write_all(b"?=\r\n")?;
                }
            }
            EncodingType::QuotedPrintable(is_ascii) => {
                for (pos, chunk) in self.text.as_bytes().chunks(76 - bytes_written).enumerate() {
                    if pos > 0 {
                        output.write_all(b"\t")?;
                    }
                    if !is_ascii {
                        output.write_all(b"=?utf-8?Q?")?;
                    } else {
                        output.write_all(b"=?us-ascii?Q?")?;
                    }
                    quoted_printable_encode(chunk, &mut output, true)?;
                    output.write_all(b"?=\r\n")?;
                }
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
