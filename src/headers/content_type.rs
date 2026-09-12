/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use super::{Header, fold::FoldWriter, rfc2047::write_parameter};
use crate::writer::Writer;
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
    fn write_header(&self, output: &mut impl Writer, column: usize) {
        let mut folder = FoldWriter::new(output, column);
        folder.write(self.c_type.as_bytes());

        if let Some((last, head)) = self.attributes.split_last() {
            for (key, value) in head {
                folder.semicolon();
                write_parameter(&mut folder, key, value, 1);
            }
            folder.semicolon();
            write_parameter(&mut folder, &last.0, &last.1, 0);
        }

        folder.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(content_type: ContentType<'_>) -> String {
        let mut output = Vec::new();
        content_type.write_header(&mut output, 14);
        String::from_utf8(output).unwrap()
    }

    fn parsed_filename(header: &str) -> String {
        use mail_parser::MimeHeaders;

        let raw = format!("Content-Type: {header}");
        let message = mail_parser::MessageParser::new()
            .parse_headers(raw.as_bytes())
            .unwrap();
        message
            .content_type()
            .and_then(|value| value.attribute("filename"))
            .unwrap()
            .to_string()
    }

    #[test]
    fn non_ascii_parameter_value_uses_rfc_2231() {
        let header =
            build(ContentType::new("attachment").attribute("filename", "Jahresabschluß, 2024.pdf"));
        assert_eq!(
            header,
            "attachment; filename*=UTF-8''Jahresabschlu%C3%9F%2C%202024.pdf\r\n"
        );
        assert_eq!(parsed_filename(&header), "Jahresabschluß, 2024.pdf");
    }

    #[test]
    fn long_rfc_2231_value_is_split_into_sections() {
        let name = "Réunion d'équipe: résumé des décisions du trimestre et prochaines étapes du projet (version finale).pdf";
        let header = build(
            ContentType::new("attachment")
                .attribute("filename", name)
                .attribute("size", "1234"),
        );
        assert!(
            header.contains("filename*0*=UTF-8''R%C3%A9union"),
            "{header:?}"
        );
        assert!(header.contains(" filename*1*="), "{header:?}");
        assert!(!header.contains("%C3;"), "{header:?}");
        assert!(header.ends_with(" size=\"1234\"\r\n"), "{header:?}");
        assert!(!header.contains("=?"), "{header:?}");
        for line in header.trim_end().split("\r\n") {
            assert!(line.len() <= 78, "{line:?}");
        }
        assert_eq!(parsed_filename(&header), name);
    }

    #[test]
    fn control_characters_in_parameter_values_are_percent_encoded() {
        let header = build(ContentType::new("attachment").attribute("filename", "a\r\nb\u{1}.pdf"));
        assert_eq!(header, "attachment; filename*=UTF-8''a%0D%0Ab%01.pdf\r\n");
    }

    #[test]
    fn plain_parameter_value_is_quoted_and_escaped() {
        let header =
            build(ContentType::new("attachment").attribute("filename", "report \"final\".pdf"));
        assert!(header.contains(r#""report \"final\".pdf""#), "{header:?}");
    }
}
