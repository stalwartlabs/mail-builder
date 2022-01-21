use std::borrow::Cow;

use super::Header;

pub struct Raw<'x> {
    pub raw: Cow<'x, str>,
}

impl<'x> Header for Raw<'x> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        header_len: usize,
    ) -> std::io::Result<usize> {
        for (pos, chunk) in self
            .raw
            .as_ref()
            .as_bytes()
            .chunks(76 - header_len)
            .enumerate()
        {
            if pos > 0 {
                output.write_all(b"\t")?;
            }
            output.write_all(chunk)?;
            output.write_all(b"\r\n")?;
        }
        Ok(0)
    }
}
