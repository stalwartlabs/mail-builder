use std::borrow::Cow;

use crate::encoders::{
    base64::base64_encode,
    encode::{get_encoding_type, EncodingType},
    quoted_printable::quoted_printable_encode,
};

use super::Header;

pub struct Text<'x> {
    pub text: Cow<'x, str>,
}

impl<'x> Header for Text<'x> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        header_len: usize,
    ) -> std::io::Result<usize> {
        let text = self.text.as_ref();
        match get_encoding_type(text, true) {
            EncodingType::Base64 => {
                for (pos, chunk) in text.as_bytes().chunks(76 - header_len).enumerate() {
                    if pos > 0 {
                        output.write_all(b"\t")?;
                    }
                    output.write_all(b"=?utf-8?B?")?;
                    base64_encode(chunk, &mut output, true)?;
                    output.write_all(b"?=\r\n")?;
                }
            }
            EncodingType::QuotedPrintable(is_ascii) => {
                for (pos, chunk) in text.as_bytes().chunks(76 - header_len).enumerate() {
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
                for (pos, chunk) in text.as_bytes().chunks(76 - header_len).enumerate() {
                    if pos > 0 {
                        output.write_all(b"\t")?;
                    }
                    output.write_all(chunk)?;
                    output.write_all(b"\r\n")?;
                }
            }
        }
        Ok(0)
    }
}
