pub enum EncodingType {
    Base64,
    QuotedPrintable(bool),
    None,
}

pub fn get_encoding_type(input: &str, is_inline: bool) -> EncodingType {
    let input = input.as_bytes();
    let base64_len = (input.len() * 4 / 3 + 3) & !3;
    let mut qp_len = if !is_inline { input.len() / 76 } else { 0 };
    let mut is_ascii = true;
    let mut needs_encoding = false;
    let mut line_len = 0;

    for (pos, &ch) in input.iter().enumerate() {
        line_len += 1;

        if ch >= 127
            || ((ch == b' ' || ch == b'\t')
                && (matches!(input.get(pos + 1..), Some([b'\n', ..] | [b'\r', b'\n', ..]))
                    || pos == input.len() - 1))
        {
            qp_len += 3;
            if !needs_encoding {
                needs_encoding = true;
            }
            if is_ascii && ch >= 127 {
                is_ascii = false;
            }
        } else if ch == b'=' || (is_inline && (ch == b'\t' || ch == b'?')) {
            qp_len += 3;
        } else {
            if ch == b'\n' {
                if !needs_encoding && line_len > 997 {
                    needs_encoding = true;
                }
                line_len = 0;
            }
            qp_len += 1;
        }
    }

    if !needs_encoding {
        EncodingType::None
    } else if qp_len < base64_len {
        EncodingType::QuotedPrintable(is_ascii)
    } else {
        EncodingType::Base64
    }
}
