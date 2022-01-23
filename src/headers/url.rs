use super::Header;

pub struct URL<'x> {
    pub url: Vec<&'x str>,
}

impl<'x> URL<'x> {
    pub fn new(url: &'x str) -> Self {
        Self { url: vec![url] }
    }

    pub fn new_list(urls: &[&'x str]) -> Self {
        Self { url: urls.to_vec() }
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
