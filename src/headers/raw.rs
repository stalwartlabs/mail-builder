use super::Header;

pub struct Raw<'x> {
    pub raw: &'x str,
}

impl<'x> Raw<'x> {
    pub fn new(raw: &'x str) -> Self {
        Self { raw }
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
