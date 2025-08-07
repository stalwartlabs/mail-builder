/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use std::io::{self, Write};

/// Encodes a single byte using the "Q" encoding from RFC 2047.
///
/// Returns the number of bytes written.
pub fn quoted_printable_encode_byte(ch: u8, output: &mut impl Write) -> io::Result<usize> {
    if ch == b'='
        || ch == b'?'
        || ch == b'_'
        || ch == b'\t'
        || ch == b'\r'
        || ch == b'\n'
        || ch >= 127
    {
        output.write_all(format!("={:02X}", ch).as_bytes())?;
        Ok(3)
    } else if ch == b' ' {
        output.write_all(b"_")?;
        Ok(1)
    } else {
        output.write_all(&[ch])?;
        Ok(1)
    }
}

/// Encodes input according using the "Q" encoding from RFC 2047.
pub fn inline_quoted_printable_encode(input: &[u8], output: &mut impl Write) -> io::Result<usize> {
    let mut bytes_written = 0;
    for &ch in input.iter() {
        bytes_written += quoted_printable_encode_byte(ch, output)?;
    }
    Ok(bytes_written)
}

pub fn quoted_printable_encode(
    input: &[u8],
    mut output: impl Write,
    is_body: bool,
) -> io::Result<usize> {
    let mut bytes_written = 0;
    if is_body {
        let mut prev_ch = 0;
        for (pos, &ch) in input.iter().enumerate() {
            if ch == b'='
                || ch >= 127
                || ((ch == b' ' || ch == b'\t')
                    && (matches!(input.get(pos + 1..), Some([b'\n', ..] | [b'\r', b'\n', ..]))
                        || (pos == input.len() - 1)))
            {
                if bytes_written + 3 > 76 {
                    output.write_all(b"=\r\n")?;
                    bytes_written = 0;
                }
                output.write_all(format!("={:02X}", ch).as_bytes())?;
                bytes_written += 3;
            } else if ch == b'\n' {
                if prev_ch != b'\r' {
                    output.write_all(b"\r\n")?;
                } else {
                    output.write_all(b"\n")?;
                }
                bytes_written = 0;
            } else {
                prev_ch = ch;
                if bytes_written + 1 > 76 {
                    output.write_all(b"=\r\n")?;
                    bytes_written = 0;
                }
                output.write_all(&[ch])?;
                bytes_written += 1;
            }
        }
    } else {
        for (pos, &ch) in input.iter().enumerate() {
            if ch == b'='
                || ch >= 127
                || (ch == b'\r' || ch == b'\n')
                || ((ch == b' ' || ch == b'\t') && (pos == input.len() - 1))
            {
                if bytes_written + 3 > 76 {
                    output.write_all(b"=\r\n")?;
                    bytes_written = 0;
                }
                output.write_all(format!("={:02X}", ch).as_bytes())?;
                bytes_written += 3;
            } else {
                if bytes_written + 1 > 76 {
                    output.write_all(b"=\r\n")?;
                    bytes_written = 0;
                }
                output.write_all(&[ch])?;
                bytes_written += 1;
            }
        }
    }
    Ok(bytes_written)
}

#[cfg(test)]
mod tests {

    #[test]
    fn encode_quoted_printable() {
        for (input, expected_result_body, expected_result_attachment, expected_result_inline) in [
            (
                "hello world".to_string(),
                "hello world",
                "hello world",
                "hello_world",
            ),
            (
                "hello_world".to_string(),
                "hello_world",
                "hello_world",
                "hello=5Fworld",
            ),
            (
                "hello ? world ?".to_string(),
                "hello ? world ?",
                "hello ? world ?",
                "hello_=3F_world_=3F",
            ),
            (
                "hello = world =".to_string(),
                "hello =3D world =3D",
                "hello =3D world =3D",
                "hello_=3D_world_=3D",
            ),
            (
                "hello\nworld\n".to_string(),
                "hello\r\nworld\r\n",
                "hello=0Aworld=0A",
                "hello=0Aworld=0A",
            ),
            (
                "hello   \nworld   \r\n   ".to_string(),
                "hello  =20\r\nworld  =20\r\n  =20",
                "hello   =0Aworld   =0D=0A  =20",
                "hello___=0Aworld___=0D=0A___",
            ),
            (
                "hello   \nworld   \n".to_string(),
                "hello  =20\r\nworld  =20\r\n",
                "hello   =0Aworld   =0A",
                "hello___=0Aworld___=0A",
            ),
            (
                "áéíóú".to_string(),
                "=C3=A1=C3=A9=C3=AD=C3=B3=C3=BA",
                "=C3=A1=C3=A9=C3=AD=C3=B3=C3=BA",
                "=C3=A1=C3=A9=C3=AD=C3=B3=C3=BA",
            ),
            (
                "안녕하세요 세계".to_string(),
                "=EC=95=88=EB=85=95=ED=95=98=EC=84=B8=EC=9A=94 =EC=84=B8=EA=B3=84",
                "=EC=95=88=EB=85=95=ED=95=98=EC=84=B8=EC=9A=94 =EC=84=B8=EA=B3=84",
                "=EC=95=88=EB=85=95=ED=95=98=EC=84=B8=EC=9A=94_=EC=84=B8=EA=B3=84",
            ),
            (
                " ".repeat(100),
                concat!(
                    "                                            ",
                    "                                =\r\n    ",
                    "                   =20"
                ),
                concat!(
                    "                                            ",
                    "                                =\r\n    ",
                    "                   =20"
                ),
                concat!(
                    "_________________________________________",
                    "_____________________________________________",
                    "______________"
                ),
            ),
        ] {
            let mut output = Vec::new();
            super::quoted_printable_encode(input.as_bytes(), &mut output, true).unwrap();
            assert_eq!(
                std::str::from_utf8(&output).unwrap(),
                expected_result_body,
                "body"
            );

            let mut output = Vec::new();
            super::quoted_printable_encode(input.as_bytes(), &mut output, false).unwrap();
            assert_eq!(
                std::str::from_utf8(&output).unwrap(),
                expected_result_attachment,
                "attachment"
            );

            let mut output = Vec::new();
            super::inline_quoted_printable_encode(input.as_bytes(), &mut output).unwrap();
            assert_eq!(
                std::str::from_utf8(&output).unwrap(),
                expected_result_inline,
                "inline"
            );
        }
    }
}
