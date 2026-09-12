/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use super::{
    base64::base64_encode_wrapped,
    quoted_printable::{
        quoted_printable_encode, split_at_safe, swar_del_or_above, swar_lane, swar_splat,
        swar_zero_lanes,
    },
};
use crate::writer::Writer;

pub(crate) enum EncodingType {
    Base64,
    QuotedPrintable(bool),
    None,
}

const BLOCK_WORDS: usize = 128;
const MAX_LINE_LEN: usize = 77;

const INLINE_CLASS: [u8; 256] = {
    let mut table = [0u8; 256];
    let mut index = 0;
    while index < table.len() {
        table[index] = match index as u8 {
            127..=u8::MAX => 3,
            b'=' | b'\r' | b'\t' | b'\n' | b'?' => 1,
            _ => 0,
        };
        index += 1;
    }
    table
};

#[inline(always)]
fn base64_len(input_len: usize) -> usize {
    (input_len.saturating_mul(4) / 3).saturating_add(3) & !3
}

#[inline(always)]
fn horizontal_sum(lanes: u64) -> usize {
    const BYTES: u64 = 0x00FF_00FF_00FF_00FF;
    const SHORTS: u64 = 0x0000_FFFF_0000_FFFF;
    let pairs = (lanes & BYTES) + ((lanes >> 8) & BYTES);
    let quads = (pairs & SHORTS) + ((pairs >> 16) & SHORTS);
    ((quads & 0xFFFF_FFFF) + (quads >> 32)) as usize
}

struct Scan {
    qp_len: usize,
    base64_len: usize,
    needs_encoding: bool,
    has_high: bool,
    bare_line_feed: bool,
    line_start: usize,
}

impl Scan {
    #[inline(always)]
    fn newline<const IS_BODY: bool>(&mut self, input: &[u8], at: usize) {
        if at + 1 - self.line_start > MAX_LINE_LEN {
            self.needs_encoding = true;
        }
        self.line_start = at + 1;

        let after_cr = input.get(at.wrapping_sub(1)) == Some(&b'\r');
        self.bare_line_feed |= !after_cr;
        if IS_BODY {
            if !after_cr {
                self.qp_len += 1;
            }
            let white = if after_cr {
                at.wrapping_sub(2)
            } else {
                at.wrapping_sub(1)
            };
            if matches!(input.get(white), Some(b' ' | b'\t')) {
                self.qp_len += 2;
                self.needs_encoding = true;
            }
        } else {
            self.qp_len += 2;
            if !after_cr {
                self.needs_encoding = true;
            }
        }
    }
}

fn scan_encoding_type<const IS_BODY: bool>(input: &[u8]) -> Scan {
    let base64_len = base64_len(input.len());
    let mut scan = Scan {
        qp_len: input.len() / 76 + input.len(),
        base64_len,
        needs_encoding: false,
        has_high: false,
        bare_line_feed: false,
        line_start: 0,
    };

    let (chunks, tail) = input.as_chunks::<8>();
    for (block_index, block) in chunks.chunks(BLOCK_WORDS).enumerate() {
        let mut escapes = 0;
        let mut high_lanes = 0;
        for (word_index, chunk) in block.iter().enumerate() {
            let word = u64::from_le_bytes(*chunk);
            let high = swar_del_or_above(word);
            let mut hits = high | swar_zero_lanes(word ^ swar_splat(b'='));
            if !IS_BODY {
                hits |= swar_zero_lanes(word ^ swar_splat(b'\r'));
            }
            escapes += hits >> 7;
            high_lanes |= high;

            let newlines = swar_zero_lanes(word ^ swar_splat(b'\n'));
            if newlines != 0 {
                let base = (block_index * BLOCK_WORDS + word_index) * 8;
                let mut bits = newlines;
                while bits != 0 {
                    scan.newline::<IS_BODY>(input, base + swar_lane(bits));
                    bits &= bits - 1;
                }
            }
        }
        scan.qp_len += 2 * horizontal_sum(escapes);
        scan.has_high |= high_lanes != 0;
        scan.needs_encoding |= scan.has_high;
        if scan.needs_encoding && scan.qp_len >= base64_len {
            return scan;
        }
    }

    let tail_start = chunks.len() * 8;
    for (offset, &ch) in tail.iter().enumerate() {
        if ch >= 127 {
            scan.has_high = true;
            scan.qp_len += 2;
        } else if ch == b'=' || (!IS_BODY && ch == b'\r') {
            scan.qp_len += 2;
        } else if ch == b'\n' {
            scan.newline::<IS_BODY>(input, tail_start + offset);
        }
    }

    scan.needs_encoding |= scan.has_high;
    if let [.., last] = input
        && matches!(last, b' ' | b'\t')
    {
        scan.qp_len += 2;
        scan.needs_encoding = true;
    }
    if input.len() - scan.line_start > MAX_LINE_LEN {
        scan.needs_encoding = true;
    }
    scan
}

impl Scan {
    #[inline(always)]
    fn encoding(&self) -> EncodingType {
        if !self.needs_encoding {
            EncodingType::None
        } else if self.qp_len < self.base64_len {
            EncodingType::QuotedPrintable(!self.has_high)
        } else {
            EncodingType::Base64
        }
    }
}

fn inline_encoding_type(input: &[u8], is_body: bool) -> EncodingType {
    let mut escapes = 0;
    let mut classes = 0;
    for &ch in input {
        let class = INLINE_CLASS[ch as usize];
        escapes += (class & 1) as usize;
        classes |= class;
    }
    let has_high = (classes & 2) != 0;
    let mut needs_encoding = has_high || input.len() > MAX_LINE_LEN;

    if let [.., last] = input
        && matches!(last, b' ' | b'\t')
    {
        needs_encoding = true;
        if *last == b' ' {
            escapes += 1;
        }
    }

    if is_body {
        let mut scanned = 0;
        while let Some(offset) = find_newline(input.get(scanned..).unwrap_or_default()) {
            let at = scanned + offset;
            let white = if input.get(at.wrapping_sub(1)) == Some(&b'\r') {
                at.wrapping_sub(2)
            } else {
                at.wrapping_sub(1)
            };
            match input.get(white) {
                Some(b' ') => {
                    escapes += 1;
                    needs_encoding = true;
                }
                Some(b'\t') => needs_encoding = true,
                _ => (),
            }
            scanned = at + 1;
        }
    }

    let qp_len = input.len() + 2 * escapes;
    if !needs_encoding {
        EncodingType::None
    } else if qp_len < base64_len(input.len()) {
        EncodingType::QuotedPrintable(!has_high)
    } else {
        EncodingType::Base64
    }
}

pub(crate) fn get_encoding_type(input: &[u8], is_inline: bool, is_body: bool) -> EncodingType {
    if is_inline {
        inline_encoding_type(input, is_body)
    } else if is_body {
        scan_encoding_type::<true>(input).encoding()
    } else {
        scan_encoding_type::<false>(input).encoding()
    }
}

fn find_newline(slice: &[u8]) -> Option<usize> {
    let (chunks, tail) = slice.as_chunks::<8>();
    for (index, chunk) in chunks.iter().enumerate() {
        let hits = swar_zero_lanes(u64::from_le_bytes(*chunk) ^ swar_splat(b'\n'));
        if hits != 0 {
            return Some(index * 8 + swar_lane(hits));
        }
    }
    tail.iter()
        .position(|&ch| ch == b'\n')
        .map(|pos| chunks.len() * 8 + pos)
}

fn write_crlf_normalized(input: &[u8], output: &mut impl Writer) {
    let mut pending = input;
    let mut scanned = 0;
    while let Some(offset) = find_newline(pending.get(scanned..).unwrap_or_default()) {
        let at = scanned + offset;
        if pending.get(at.wrapping_sub(1)) == Some(&b'\r') {
            scanned = at + 1;
        } else {
            let (head, tail) = split_at_safe(pending, at);
            if !head.is_empty() {
                output.write(head);
            }
            output.write(b"\r\n");
            pending = tail.get(1..).unwrap_or_default();
            scanned = 0;
        }
    }
    if !pending.is_empty() {
        output.write(pending);
    }
}

/// Writes the `Content-Transfer-Encoding` header, the blank line and the
/// encoded body for `input`, choosing the cheapest valid encoding.
pub(crate) fn write_encoded_body(input: &[u8], output: &mut impl Writer, is_body: bool) {
    let scan = if is_body {
        scan_encoding_type::<true>(input)
    } else {
        scan_encoding_type::<false>(input)
    };
    match scan.encoding() {
        EncodingType::Base64 => {
            output.write(b"Content-Transfer-Encoding: base64\r\n\r\n");
            base64_encode_wrapped(input, output);
        }
        EncodingType::QuotedPrintable(_) => {
            output.write(b"Content-Transfer-Encoding: quoted-printable\r\n\r\n");
            quoted_printable_encode(input, output, is_body);
        }
        EncodingType::None => {
            output.write(b"Content-Transfer-Encoding: 7bit\r\n\r\n");
            if is_body && scan.bare_line_feed {
                write_crlf_normalized(input, output);
            } else {
                output.write(input);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_encoded_body() {
        let mut output = Vec::new();
        write_encoded_body(b"a b c\r\n", &mut output, false);
        assert_eq!(output, b"Content-Transfer-Encoding: 7bit\r\n\r\na b c\r\n");

        let mut output = Vec::new();
        write_encoded_body(
            b"a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a\r\n",
            &mut output,
            false,
        );
        assert_eq!(output, b"Content-Transfer-Encoding: 7bit\r\n\r\na a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a\r\n");

        let long_line =
            "a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a a";
        let mut output = Vec::new();
        write_encoded_body(format!("{long_line}\r\n").as_bytes(), &mut output, false);
        let expected = format!(
            "Content-Transfer-Encoding: quoted-printable\r\n\r\n{}=\r\n{}=0D=0A",
            &long_line[..75],
            &long_line[75..]
        );
        assert_eq!(output, expected.as_bytes());

        let mut output = Vec::new();
        let long_line = "a".repeat(100);
        write_encoded_body(long_line.as_bytes(), &mut output, false);
        let expected = format!(
            "Content-Transfer-Encoding: quoted-printable\r\n\r\n{}",
            "a".repeat(75) + "=\r\n" + &"a".repeat(25)
        );
        assert_eq!(output, expected.as_bytes());

        let mut output = Vec::new();
        write_encoded_body(b"one\ntwo\r\nthree\n", &mut output, true);
        assert_eq!(
            output,
            b"Content-Transfer-Encoding: 7bit\r\n\r\none\r\ntwo\r\nthree\r\n"
        );
    }
}
