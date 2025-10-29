/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use std::io::{self, Write};

use super::{base64::base64_encode_mime, quoted_printable::inline_quoted_printable_encode};

pub enum EncodingType {
    Base64,
    QuotedPrintable(bool),
    None,
}

pub(crate) fn get_encoding_type(input: &[u8], is_inline: bool, is_body: bool) -> EncodingType {
    let base64_len = (input.len() * 4 / 3 + 3) & !3;
    let mut qp_len = if !is_inline { input.len() / 76 } else { 0 };
    let mut is_ascii = true;
    let mut needs_encoding = false;
    let mut line_len = 0;
    let mut prev_ch = 0;

    for (pos, &ch) in input.iter().enumerate() {
        line_len += 1;

        if ch >= 127
            || ((ch == b' ' || ch == b'\t')
                && ((is_body
                    && matches!(input.get(pos + 1..), Some([b'\n', ..] | [b'\r', b'\n', ..])))
                    || pos == input.len() - 1))
        {
            qp_len += 3;
            if !needs_encoding {
                needs_encoding = true;
            }
            if is_ascii && ch >= 127 {
                is_ascii = false;
            }
        } else if ch == b'='
            || (!is_body && ch == b'\r')
            || (is_inline && (ch == b'\t' || ch == b'\r' || ch == b'\n' || ch == b'?'))
        {
            qp_len += 3;
        } else if ch == b'\n' {
            if !needs_encoding && line_len > 77 {
                needs_encoding = true;
            }
            if is_body {
                if prev_ch != b'\r' {
                    qp_len += 1;
                }
                qp_len += 1;
            } else {
                if !needs_encoding && prev_ch != b'\r' {
                    needs_encoding = true;
                }
                qp_len += 3;
            }
            line_len = 0;
        } else {
            qp_len += 1;
        }

        prev_ch = ch;
    }

    if !needs_encoding && line_len > 77 {
        needs_encoding = true;
    }

    if !needs_encoding {
        EncodingType::None
    } else if qp_len < base64_len {
        EncodingType::QuotedPrintable(is_ascii)
    } else {
        EncodingType::Base64
    }
}

pub(crate) fn rfc2047_encode(input: &str, mut output: impl Write) -> io::Result<usize> {
    Ok(match get_encoding_type(input.as_bytes(), true, false) {
        EncodingType::Base64 => {
            output.write_all(b"\"=?utf-8?B?")?;
            let bytes_written = base64_encode_mime(input.as_bytes(), &mut output, true)? + 14;
            output.write_all(b"?=\"")?;
            bytes_written
        }
        EncodingType::QuotedPrintable(is_ascii) => {
            if !is_ascii {
                output.write_all(b"\"=?utf-8?Q?")?;
            } else {
                output.write_all(b"\"=?us-ascii?Q?")?;
            }
            let bytes_written = inline_quoted_printable_encode(input.as_bytes(), &mut output)?
                + if is_ascii { 19 } else { 14 };
            output.write_all(b"?=\"")?;
            bytes_written
        }
        EncodingType::None => {
            let mut bytes_written = 2;
            output.write_all(b"\"")?;
            for &ch in input.as_bytes() {
                if ch == b'\\' || ch == b'"' {
                    output.write_all(b"\\")?;
                    bytes_written += 1;
                } else if ch == b'\r' || ch == b'\n' {
                    continue;
                }
                output.write_all(&[ch])?;
                bytes_written += 1;
            }
            output.write_all(b"\"")?;
            bytes_written
        }
    })
}


/// RFC 2231, extended-other-values encoding
pub(crate) fn encode_extended_other_values(
    input: &[u8],
    mut output: impl Write,
) -> io::Result<usize> {
    let mut bytes_written = 0;

    for &b in input {
        if byte_needs_ext_escaping(b) {
            output.write_all(&[b'%', HEX[(b >> 4) as usize], HEX[(b & 0x0F) as usize]])?;
            bytes_written += 3;
        } else {
            output.write_all(&[b])?;
            bytes_written += 1;
        }
    }

    Ok(bytes_written)
}

/// Writes MIME parameter value as qcontent with RFC 5322 escaping
pub(crate) fn encode_qcontent(input: &[u8], mut output: impl Write) -> io::Result<usize> {
    let mut bytes_written = 0;

    for &b in input {
        if b == b'\t' || b == b'\\' || b == b'"' {
            output.write_all(b"\\")?;
            bytes_written += 1;
        }

        output.write_all(&[b])?;
        bytes_written += 1;
    }

    Ok(bytes_written)
}

/// Returns true when the string can be used in a quoted-string with or without escaping
pub(crate) fn is_text(key: &str) -> bool {
    for &b in key.as_bytes() {
        // Use RFC 5322 version of just VCHAR / WSP as it is more conservative than RFC 2822 text
        if b < 8 || (b >= 10 && b <= 31) || b >= 127 {
            return false;
        }
    }
    true
}

/// Write a MIME parameter to `output`` with RFC 2231 parameter encoding, returning the number of
/// bytes written on the last line.
///
/// Takes `bytes_written`` to specify how many bytes have already been written on the current line
///
/// Will encode either with escaped quoted text (qtext) or extended parameter encoding and fold
/// over multiple lines as necessary. This will always write key before wrapping.
pub(crate) fn rfc2231_encode_parameter(
    key: &str,
    value: &str,
    mut output: impl Write,
    bytes_written: usize,
) -> io::Result<usize> {
    let mut bytes_written = bytes_written;
    let extended_encoding = !is_text(value);

    let split_point = if extended_encoding {
        floor_ext_encoding_boundary(
            value,
            MAX_RFC2231_LINE_LENGTH - bytes_written - key.len() - 9,
        )
    } else {
        floor_quoted_encoding_boundary(
            value,
            MAX_RFC2231_LINE_LENGTH - bytes_written - key.len() - 3,
        )
    };

    if split_point == value.len() {
        if extended_encoding {
            output.write_all(key.as_bytes())?;
            output.write_all(b"*=utf-8''")?;
            bytes_written += encode_extended_other_values(value.as_bytes(), &mut output)?;
            bytes_written += key.len() + 9;
        } else {
            output.write_all(key.as_bytes())?;
            output.write_all(b"=\"")?;
            bytes_written += encode_qcontent(value.as_bytes(), &mut output)?;
            output.write_all(b"\"")?;
            bytes_written += key.len() + 3;
        }
        return Ok(bytes_written);
    }

    let mut section_num: usize = 0;
    let mut remainder = value;

    output.write_all(key.as_bytes())?;
    bytes_written += key.len();

    loop {
        if section_num != 0 {
            output.write_all(b";\r\n\t")?;
            output.write_all(key.as_bytes())?;
            bytes_written = key.len() + 1;
        }

        let section = format!("*{}", section_num);
        output.write_all(section.as_bytes())?;
        bytes_written += section.len();

        if extended_encoding {
            output.write_all(b"*=")?;
            bytes_written += 2;

            if section_num == 0 {
                output.write_all(b"utf-8''")?;
                bytes_written += 7;
            }

            if MAX_RFC2231_LINE_LENGTH > bytes_written {
                let split_point = floor_ext_encoding_boundary(&remainder, MAX_RFC2231_LINE_LENGTH - bytes_written);

                let (current_section, rest) = remainder.split_at(split_point);
                bytes_written += encode_extended_other_values(current_section.as_bytes(), &mut output)?;
                remainder = rest;
            }
        } else {
            output.write_all(b"=\"")?;
            bytes_written += 2;

            if MAX_RFC2231_LINE_LENGTH - 2 > bytes_written {
                let split_point = floor_quoted_encoding_boundary(&remainder, MAX_RFC2231_LINE_LENGTH - 2 - bytes_written);
                let (current_section, rest) = remainder.split_at(split_point);
                bytes_written += encode_qcontent(current_section.as_bytes(), &mut output)?;
                remainder = rest;
            }

            output.write_all(b"\"")?;
            bytes_written += 1;
        }

        if remainder.len() == 0 {
            break;
        }

        section_num += 1;
    }

    Ok(bytes_written)
}

/// Returns true if the character needs to be escaped in extended-other-values
#[inline]
pub(crate) fn needs_ext_escaping(c: char) -> bool {
    !matches!(c, '!' | '#'..='$' | '&' | '+' | '-'..='.' | '0'..='9' | 'A'..='Z' | '^'..='~')
}

/// Returns true if the byte needs to be escaped in extended-other-values
#[inline]
pub(crate) fn byte_needs_ext_escaping(b: u8) -> bool {
    !matches!(b, b'!' | b'#'..=b'$' | b'&' | b'+' | b'-'..=b'.' | b'0'..=b'9' | b'A'..=b'Z' | b'^'..=b'~')
}

/// Returns the last index `x` not exceeding `length` where [`needs_ext_escaping(x)`] is `false`.
pub(crate) fn floor_ext_encoding_boundary(s: &str, length: usize) -> usize {
    let mut encode_len = 0;
    let mut last_idx: usize = 0;

    let mut chars = s.char_indices();

    for (_idx, c) in &mut chars {
        if needs_ext_escaping(c) {
            encode_len += c.len_utf8() * 3;
        } else {
            encode_len += 1;
        }

        if encode_len > length {
            break;
        }

        last_idx += c.len_utf8();
    }

    last_idx
}

/// Returns true if the byte needs to be escaped in a quoted-string
#[inline]
pub(crate) fn needs_quoted_char(b: u8) -> bool {
    matches!(b, b'\t' | b'\\' | b'"')
}

/// Returns the last index `x` not exceeding `length` where [`needs_quoted_char(x)`] is `false`.
///
/// Only pass in ASCII
pub(crate) fn floor_quoted_encoding_boundary(s: &str, length: usize) -> usize {
    let mut encode_len = 0;
    let s = s.as_bytes();
    let mut idx: usize = 0;

    while idx < s.len() {
        if needs_quoted_char(s[idx]) {
            encode_len += 2;
        } else {
            encode_len += 1;
        }

        if encode_len > length {
            break;
        }

        idx += 1;
    }
    idx
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::{encode_extended_other_values, encode_qcontent, rfc2231_encode_parameter};
    use std::str::from_utf8;

    #[test]
    fn test_ext_boundary() {
        let s = "1234567890测试文本.doc";
        let x = super::floor_ext_encoding_boundary(s, 50);
        let (a, b) = s.split_at(x);
        let mut output = Vec::new();
        encode_extended_other_values(a.as_bytes(), &mut output).unwrap();
        println!(
            "'{}' ({}) '{}'",
            a,
            from_utf8(&output).unwrap(),
            b
        );
    }

    #[test]
    fn test_quoted_boundary() {
        let s = "hello \"world\".doc";
        let x = super::floor_quoted_encoding_boundary(s, 8);
        let (a, b) = s.split_at(x);
        let mut output = Vec::new();
        encode_qcontent(a.as_bytes(), &mut output).unwrap();
        println!(
            "'{}' ({}) '{}'",
            a,
            from_utf8(&output).unwrap(),
            b
        );
    }

    #[test]
    fn test_encode_extended_other_values() {
        for (input, expected_result) in [
            ("Test".to_string(), "Test"),
            ("Ye ".to_string(), "Ye%20"),
            (
                "Are you a Shimano or Campagnolo person?".to_string(),
                "Are%20you%20a%20Shimano%20or%20Campagnolo%20person%3F",
            ),
            (
                "<!DOCTYPE html>\n<html>\n<body>\n</body>\n</html>\n".to_string(),
                "%3C!DOCTYPE%20html%3E%0A%3Chtml%3E%0A%3Cbody%3E%0A%3C%2Fbody%3E%0A%3C%2Fhtml%3E%0A",
            ),
            ("áéíóú".to_string(), "%C3%A1%C3%A9%C3%AD%C3%B3%C3%BA"),
        ] {
            let mut output = Vec::new();
            encode_extended_other_values(input.as_bytes(), &mut output).unwrap();
            assert_eq!(from_utf8(&output).unwrap(), expected_result);
        }
    }

    #[test]
    fn test_encode_rfc2231() {
        for (key, value, expected_result, expected_size) in [
            // test wrapping on utf-8 sequence, this will naturally break on the % of the second byte of the multi-byte sequence
            (
                "filename",
                "123456789012345678901234567890123456789012345678 测试文本.doc",
                "filename*0*=utf-8''123456789012345678901234567890123456789012345678%20;\r\n\tfilename*1*=%E6%B5%8B%E8%AF%95%E6%96%87%E6%9C%AC.doc",
                52,
            ),
            (
                "filename",
                "1234567890123456789012345678901234567890123456 测试文本.doc",
                "filename*0*=utf-8''1234567890123456789012345678901234567890123456%20;\r\n\tfilename*1*=%E6%B5%8B%E8%AF%95%E6%96%87%E6%9C%AC.doc",
                52,
            ),
            (
                "filename",
                "12345678901234567890123456789012345678901234 测试文本.doc",
                "filename*0*=utf-8''12345678901234567890123456789012345678901234%20;\r\n\tfilename*1*=%E6%B5%8B%E8%AF%95%E6%96%87%E6%9C%AC.doc",
                52,
            ),
            (
                "filename",
                "1234567890123456789012345678901234567890123 测试文本.doc",
                "filename*0*=utf-8''1234567890123456789012345678901234567890123%20%E6%B5%8B;\r\n\tfilename*1*=%E8%AF%95%E6%96%87%E6%9C%AC.doc",
                43,
            ),
            (
                "filename",
                "1234567890123456789012345678901234567890 测试文本.doc",
                "filename*0*=utf-8''1234567890123456789012345678901234567890%20%E6%B5%8B;\r\n\tfilename*1*=%E8%AF%95%E6%96%87%E6%9C%AC.doc",
                43,
            ),
            (
                "filename",
                r##"x!"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ[\]^_`abcdefghijklmnopqrstuvwxyz{|}~.txt"##,
                "filename*0=\"x!\\\"#$%&'()*+,-./0123456789:;<=>?@ABCDEFGHIJKLMNOPQRSTUVWXYZ\";\r\n\tfilename*1=\"[\\\\]^_`abcdefghijklmnopqrstuvwxyz{|}~.txt\"",
                54,
            ),
            // test when the wrapping wants to happen on a backslash
            (
                "filename",
                r##"12345678901234567890123456789012345678901234567890123456789\0123456789.txt"##,
                "filename*0=\"12345678901234567890123456789012345678901234567890123456789\";\r\n\tfilename*1=\"\\\\0123456789.txt\"",
                29,
            ),
            (
                "filename",
                r##"\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\.txt"##,
                "filename*0=\"\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\\";\r\n\tfilename*1=\"\\\\\\\\\\\\.txt\"",
                23,
            ),
            ("filename", "file.txt", "filename=\"file.txt\"", 19),
            (
                "filename",
                "测试文本.doc",
                "filename*=utf-8''%E6%B5%8B%E8%AF%95%E6%96%87%E6%9C%AC.doc",
                57,
            ),
            (
                "filename",
                "tps\x07-\x08report.doc",
                "filename*=utf-8''tps%07-%08report.doc",
                37,
            ),
        ] {
            let mut output = Vec::new();
            let line_size = rfc2231_encode_parameter(key, value, &mut output, 1).unwrap();
            assert_eq!(from_utf8(&output).unwrap(), expected_result);
            assert_eq!(line_size - 1, expected_size);
        }
    }
}

// This ensures we always have space for CRLF + WSP
const MAX_RFC2231_LINE_LENGTH: usize = 75;

const HEX: &[u8] = b"0123456789ABCDEF";