/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

pub mod base64;
pub mod encode;
pub mod quoted_printable;
pub use base64::Base64Encoder;
pub use quoted_printable::QuotedPrintableEncoder;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64_encoder_does_not_wrap_by_default() {
        let encoded = Base64Encoder::new().encode(&[b'x'; 300]).unwrap();
        assert!(!encoded.contains(&b'\n'), "unexpected line break");
    }

    #[test]
    fn base64_encoder_wraps_lines_when_requested() {
        let encoded = Base64Encoder::new()
            .wrap_lines()
            .encode(&[b'x'; 300])
            .unwrap();
        assert!(encoded.windows(2).any(|w| w == b"\r\n"), "no line break");
        for line in encoded.split(|&ch| ch == b'\n') {
            assert!(line.len() <= 77, "line too long: {}", line.len());
        }
    }

    #[test]
    fn quoted_printable_encoder_escapes_line_breaks_by_default() {
        let encoded = QuotedPrintableEncoder::new().encode(b"a\r\nb").unwrap();
        assert_eq!(encoded, b"a=0D=0Ab");
    }

    #[test]
    fn quoted_printable_encoder_preserves_line_breaks_when_requested() {
        let encoded = QuotedPrintableEncoder::new()
            .preserve_line_breaks()
            .encode(b"a\r\nb")
            .unwrap();
        assert_eq!(encoded, b"a\r\nb");
    }
}
