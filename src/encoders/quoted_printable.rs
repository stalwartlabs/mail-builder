use std::io::{self, Write};

pub fn quoted_printable_encode(
    input: &[u8],
    mut output: impl Write,
    is_inline: bool,
) -> io::Result<usize> {
    let mut bytes_written = 0;
    if !is_inline {
        for (pos, &ch) in input.iter().enumerate() {
            if ch == b'='
                || ch >= 127
                || ((ch == b' ' || ch == b'\t')
                    && (matches!(input.get(pos + 1..), Some([b'\n', ..] | [b'\r', b'\n', ..]))
                        || pos == input.len() - 1))
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
    } else {
        for &ch in input.iter() {
            if ch == b'=' || ch == b'?' || ch == b'\t' || ch == b'\r' || ch == b'\n' || ch >= 127 {
                output.write_all(format!("={:02X}", ch).as_bytes())?;
                bytes_written += 3;
            } else if ch == b' ' {
                output.write_all(b"_")?;
                bytes_written += 1;
            } else {
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
        for (input, expected_result, expected_result_inline) in [
            ("hello world".to_string(), "hello world", "hello_world"),
            (
                "hello ? world ?".to_string(),
                "hello ? world ?",
                "hello_=3F_world_=3F",
            ),
            (
                "hello = world =".to_string(),
                "hello =3D world =3D",
                "hello_=3D_world_=3D",
            ),
            (
                "hello\nworld\n".to_string(),
                "hello\nworld\n",
                "hello=0Aworld=0A",
            ),
            (
                "hello   \nworld   \r\n   ".to_string(),
                "hello  =20\nworld  =20\r\n  =20",
                "hello___=0Aworld___=0D=0A___",
            ),
            (
                "hello   \nworld   \n".to_string(),
                "hello  =20\nworld  =20\n",
                "hello___=0Aworld___=0A",
            ),
            (
                "áéíóú".to_string(),
                "=C3=A1=C3=A9=C3=AD=C3=B3=C3=BA",
                "=C3=A1=C3=A9=C3=AD=C3=B3=C3=BA",
            ),
            (
                "안녕하세요 세계".to_string(),
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
                    "_________________________________________",
                    "_____________________________________________",
                    "______________"
                ),
            ),
        ] {
            let mut output = Vec::new();
            super::quoted_printable_encode(input.as_bytes(), &mut output, false).unwrap();
            assert_eq!(std::str::from_utf8(&output).unwrap(), expected_result);
            let mut output = Vec::new();
            super::quoted_printable_encode(input.as_bytes(), &mut output, true).unwrap();
            assert_eq!(
                std::str::from_utf8(&output).unwrap(),
                expected_result_inline
            );
        }
    }
}
