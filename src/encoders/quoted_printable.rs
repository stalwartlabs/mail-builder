/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use crate::writer::{IoWriter, Writer};
use std::io::{self, Write};

const HEX_DIGITS: &[u8; 16] = b"0123456789ABCDEF";
const HEX_WORDS: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut index = 0;
    while index < table.len() {
        table[index] =
            u32::from_le_bytes([b'=', HEX_DIGITS[index >> 4], HEX_DIGITS[index & 0x0F], 0]);
        index += 1;
    }
    table
};

const MAX_LINE_LEN: usize = 76;
const MAX_CONTENT_LEN: usize = MAX_LINE_LEN - 1;
const SOFT_BREAK: &[u8; 3] = b"=\r\n";
const HARD_BREAK: &[u8; 2] = b"\r\n";
const ESCAPE_LINE_LEN: usize = MAX_CONTENT_LEN + 3;
const ESCAPE_BUFFER_LEN: usize = ESCAPE_LINE_LEN * 4 + 1;
const SMALL_ESCAPES: usize = 8;

const SWAR_ONES: u64 = 0x0101_0101_0101_0101;
const SWAR_SIGNS: u64 = 0x8080_8080_8080_8080;
const SWAR_LOW7: u64 = 0x7F7F_7F7F_7F7F_7F7F;

#[inline(always)]
pub(crate) const fn swar_splat(byte: u8) -> u64 {
    SWAR_ONES.wrapping_mul(byte as u64)
}

#[inline(always)]
const fn swar_first_zero(word: u64) -> u64 {
    word.wrapping_sub(SWAR_ONES) & !word & SWAR_SIGNS
}

#[inline(always)]
pub(crate) const fn swar_zero_lanes(word: u64) -> u64 {
    !(((word & SWAR_LOW7).wrapping_add(SWAR_LOW7)) | word) & SWAR_SIGNS
}

#[inline(always)]
pub(crate) const fn swar_del_or_above(word: u64) -> u64 {
    (((word & SWAR_LOW7).wrapping_add(SWAR_ONES)) | word) & SWAR_SIGNS
}

#[inline(always)]
pub(crate) const fn swar_lane(hits: u64) -> usize {
    (hits.trailing_zeros() / 8) as usize
}

#[inline(always)]
pub(crate) fn split_at_safe(slice: &[u8], mid: usize) -> (&[u8], &[u8]) {
    slice.split_at_checked(mid).unwrap_or((slice, &[]))
}

const CLASS_PLAIN: u8 = 0;
const CLASS_SPACE: u8 = 1;
const CLASS_ESCAPE: u8 = 2;

const Q_UNSTRUCTURED: [u8; 256] = {
    let mut table = [CLASS_PLAIN; 256];
    let mut index = 0;
    while index < table.len() {
        table[index] = match index as u8 {
            b'=' | b'?' | b'_' | b'\t' | b'\r' | b'\n' | 127..=u8::MAX => CLASS_ESCAPE,
            b' ' => CLASS_SPACE,
            _ => CLASS_PLAIN,
        };
        index += 1;
    }
    table
};

const Q_PHRASE: [u8; 256] = {
    let mut table = [CLASS_PLAIN; 256];
    let mut index = 0;
    while index < table.len() {
        table[index] = match index as u8 {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'!' | b'*' | b'+' | b'-' | b'/' => {
                CLASS_PLAIN
            }
            b' ' => CLASS_SPACE,
            _ => CLASS_ESCAPE,
        };
        index += 1;
    }
    table
};

#[inline(always)]
fn escape_triplet(ch: u8) -> [u8; 3] {
    let [escape, high, low, _] = HEX_WORDS[ch as usize].to_le_bytes();
    [escape, high, low]
}

#[inline]
fn write_escaped(ch: u8, output: &mut impl Writer) {
    output.write(&escape_triplet(ch));
}

/// Encodes a single byte using the "Q" encoding from RFC 2047.
///
/// Returns the number of bytes written.
#[inline]
pub(crate) fn quoted_printable_encode_byte(ch: u8, output: &mut impl Writer) -> usize {
    q_encode_byte(ch, Q_UNSTRUCTURED[ch as usize], output)
}

#[inline]
pub(crate) fn quoted_printable_encode_phrase_byte(ch: u8, output: &mut impl Writer) -> usize {
    q_encode_byte(ch, Q_PHRASE[ch as usize], output)
}

#[inline(always)]
fn q_encode_byte(ch: u8, class: u8, output: &mut impl Writer) -> usize {
    match class {
        CLASS_PLAIN => {
            output.write_byte(ch);
            1
        }
        CLASS_SPACE => {
            output.write_byte(b'_');
            1
        }
        _ => {
            write_escaped(ch, output);
            3
        }
    }
}

#[inline]
pub(crate) const fn quoted_printable_byte_len(ch: u8) -> usize {
    if Q_UNSTRUCTURED[ch as usize] == CLASS_ESCAPE {
        3
    } else {
        1
    }
}

#[inline]
pub(crate) const fn quoted_printable_phrase_byte_len(ch: u8) -> usize {
    if Q_PHRASE[ch as usize] == CLASS_ESCAPE {
        3
    } else {
        1
    }
}

#[inline(always)]
fn write_underscores(count: usize, output: &mut impl Writer) {
    const UNDERSCORES: [u8; 32] = [b'_'; 32];
    let mut left = count;
    while left > 0 {
        let take = left.min(UNDERSCORES.len());
        output.write(UNDERSCORES.get(..take).unwrap_or_default());
        left -= take;
    }
}

#[inline(always)]
fn fill_escapes(region: &mut [u8], run: &[u8]) {
    for (index, &ch) in run.iter().enumerate() {
        let word = HEX_WORDS[ch as usize].to_le_bytes();
        if let Some(slot) = region.get_mut(index * 3..index * 3 + 4) {
            slot.copy_from_slice(&word);
        } else if let Some(slot) = region.get_mut(index * 3..index * 3 + 3) {
            slot.copy_from_slice(word.get(..3).unwrap_or_default());
        }
    }
}

#[inline(always)]
fn write_small_escapes(run: &[u8], output: &mut impl Writer) {
    debug_assert!(run.len() <= SMALL_ESCAPES);
    let mut buffer = [0u8; SMALL_ESCAPES * 3 + 1];
    fill_escapes(&mut buffer, run);
    output.write(buffer.get(..run.len() * 3).unwrap_or_default());
}

fn write_escape_run(mut run: &[u8], output: &mut impl Writer) {
    while !run.is_empty() {
        let (head, tail) = split_at_safe(run, run.len().min(SMALL_ESCAPES));
        write_small_escapes(head, output);
        run = tail;
    }
}

#[inline(always)]
fn q_encode(input: &[u8], table: &[u8; 256], output: &mut impl Writer) -> usize {
    let mut bytes_written = 0;
    let mut rest = input;
    while let [first, ..] = rest {
        match table[*first as usize] {
            CLASS_PLAIN => {
                let end = rest
                    .iter()
                    .position(|&ch| table[ch as usize] != CLASS_PLAIN)
                    .unwrap_or(rest.len());
                let (run, tail) = split_at_safe(rest, end);
                output.write(run);
                bytes_written += end;
                rest = tail;
            }
            CLASS_SPACE => {
                let end = rest.iter().position(|&ch| ch != b' ').unwrap_or(rest.len());
                write_underscores(end, output);
                bytes_written += end;
                rest = split_at_safe(rest, end).1;
            }
            _ => {
                let end = rest
                    .iter()
                    .position(|&ch| table[ch as usize] != CLASS_ESCAPE)
                    .unwrap_or(rest.len());
                let (run, tail) = split_at_safe(rest, end);
                write_escape_run(run, output);
                bytes_written += end * 3;
                rest = tail;
            }
        }
    }
    bytes_written
}

pub(crate) fn phrase_quoted_printable_encode(input: &[u8], output: &mut impl Writer) -> usize {
    q_encode(input, &Q_PHRASE, output)
}

/// Encodes input according using the "Q" encoding from RFC 2047.
pub(crate) fn inline_quoted_printable_encode(input: &[u8], output: &mut impl Writer) -> usize {
    q_encode(input, &Q_UNSTRUCTURED, output)
}

#[inline(always)]
const fn is_body_stop(ch: u8) -> bool {
    ch >= 127 || ch == b'=' || ch == b'\n'
}

#[inline(always)]
const fn is_body_escape(ch: u8) -> bool {
    ch >= 127 || ch == b'='
}

#[inline(always)]
const fn is_attachment_escape(ch: u8) -> bool {
    ch >= 127 || ch == b'=' || ch == b'\n' || ch == b'\r'
}

fn find_body_stop(slice: &[u8]) -> Option<usize> {
    let (chunks, tail) = slice.as_chunks::<8>();
    for (index, chunk) in chunks.iter().enumerate() {
        let word = u64::from_le_bytes(*chunk);
        let hits = swar_del_or_above(word)
            | swar_first_zero(word ^ swar_splat(b'='))
            | swar_first_zero(word ^ swar_splat(b'\n'));
        if hits != 0 {
            return Some(index * 8 + swar_lane(hits));
        }
    }
    tail.iter()
        .position(|&ch| is_body_stop(ch))
        .map(|pos| chunks.len() * 8 + pos)
}

fn find_attachment_escape(slice: &[u8]) -> Option<usize> {
    let (chunks, tail) = slice.as_chunks::<8>();
    for (index, chunk) in chunks.iter().enumerate() {
        let word = u64::from_le_bytes(*chunk);
        let hits = swar_del_or_above(word)
            | swar_first_zero(word ^ swar_splat(b'='))
            | swar_first_zero(word ^ swar_splat(b'\n'))
            | swar_first_zero(word ^ swar_splat(b'\r'));
        if hits != 0 {
            return Some(index * 8 + swar_lane(hits));
        }
    }
    tail.iter()
        .position(|&ch| is_attachment_escape(ch))
        .map(|pos| chunks.len() * 8 + pos)
}

fn body_escape_len(slice: &[u8]) -> usize {
    let (chunks, tail) = slice.as_chunks::<8>();
    for (index, chunk) in chunks.iter().enumerate() {
        let word = u64::from_le_bytes(*chunk);
        let plain =
            (swar_del_or_above(word) | swar_zero_lanes(word ^ swar_splat(b'='))) ^ SWAR_SIGNS;
        if plain != 0 {
            return index * 8 + swar_lane(plain);
        }
    }
    chunks.len() * 8
        + tail
            .iter()
            .position(|&ch| !is_body_escape(ch))
            .unwrap_or(tail.len())
}

fn attachment_escape_len(slice: &[u8]) -> usize {
    let (chunks, tail) = slice.as_chunks::<8>();
    for (index, chunk) in chunks.iter().enumerate() {
        let word = u64::from_le_bytes(*chunk);
        let plain = (swar_del_or_above(word)
            | swar_zero_lanes(word ^ swar_splat(b'='))
            | swar_zero_lanes(word ^ swar_splat(b'\n'))
            | swar_zero_lanes(word ^ swar_splat(b'\r')))
            ^ SWAR_SIGNS;
        if plain != 0 {
            return index * 8 + swar_lane(plain);
        }
    }
    chunks.len() * 8
        + tail
            .iter()
            .position(|&ch| !is_attachment_escape(ch))
            .unwrap_or(tail.len())
}

#[inline(always)]
fn write_plain(mut run: &[u8], output: &mut impl Writer, mut column: usize) -> usize {
    while column + run.len() > MAX_CONTENT_LEN {
        let Some((head, tail)) = run.split_at_checked(MAX_CONTENT_LEN.saturating_sub(column))
        else {
            break;
        };
        if !head.is_empty() {
            output.write(head);
        }
        output.write(SOFT_BREAK);
        column = 0;
        run = tail;
    }
    if !run.is_empty() {
        output.write(run);
    }
    column + run.len()
}

#[inline(always)]
fn write_escape(ch: u8, output: &mut impl Writer, column: usize) -> usize {
    let [_, high, low] = escape_triplet(ch);
    if column + 3 > MAX_CONTENT_LEN {
        output.write(&[b'=', b'\r', b'\n', b'=', high, low]);
        3
    } else {
        output.write(&[b'=', high, low]);
        column + 3
    }
}

#[inline(always)]
fn write_escapes(
    run: &[u8],
    output: &mut impl Writer,
    column: usize,
    buffer: &mut [u8; ESCAPE_BUFFER_LEN],
) -> usize {
    if run.len() <= SMALL_ESCAPES && column + run.len() * 3 <= MAX_CONTENT_LEN {
        if run.is_empty() {
            return column;
        }
        write_small_escapes(run, output);
        return column + run.len() * 3;
    }
    write_escapes_wrapped(run, output, column, buffer)
}

fn write_escapes_wrapped(
    mut run: &[u8],
    output: &mut impl Writer,
    mut column: usize,
    buffer: &mut [u8; ESCAPE_BUFFER_LEN],
) -> usize {
    let mut filled = 0;
    while !run.is_empty() {
        if column + 3 > MAX_CONTENT_LEN {
            if let Some(region) = buffer.get_mut(filled..filled + SOFT_BREAK.len()) {
                region.copy_from_slice(SOFT_BREAK);
                filled += SOFT_BREAK.len();
            }
            column = 0;
        }
        let (head, tail) = split_at_safe(run, run.len().min((MAX_CONTENT_LEN - column) / 3));
        if let Some(region) = buffer.get_mut(filled..filled + head.len() * 3 + 1) {
            fill_escapes(region, head);
            filled += head.len() * 3;
        }
        column += head.len() * 3;
        run = tail;
        if filled + ESCAPE_LINE_LEN > ESCAPE_BUFFER_LEN {
            output.write(buffer.get(..filled).unwrap_or_default());
            filled = 0;
        }
    }
    if filled > 0 {
        output.write(buffer.get(..filled).unwrap_or_default());
    }
    column
}

#[inline(always)]
fn write_run_with_trailing_ws(run: &[u8], output: &mut impl Writer, column: usize) -> usize {
    match run {
        [head @ .., last @ (b' ' | b'\t')] => {
            let column = write_plain(head, output, column);
            write_escape(*last, output, column)
        }
        _ => write_plain(run, output, column),
    }
}

fn encode_body(input: &[u8], output: &mut impl Writer) -> usize {
    let mut buffer = [0u8; ESCAPE_BUFFER_LEN];
    let mut column = 0;
    let mut rest = input;
    loop {
        let Some(stop) = find_body_stop(rest) else {
            return write_run_with_trailing_ws(rest, output, column);
        };
        let (run, tail) = split_at_safe(rest, stop);
        match tail {
            [b'\n', after @ ..] => {
                let run = match run {
                    [head @ .., b'\r'] => head,
                    _ => run,
                };
                write_run_with_trailing_ws(run, output, column);
                output.write(HARD_BREAK);
                column = 0;
                rest = after;
            }
            _ => {
                column = write_plain(run, output, column);
                let (escapes, next) = split_at_safe(tail, body_escape_len(tail));
                column = write_escapes(escapes, output, column, &mut buffer);
                rest = next;
            }
        }
    }
}

fn encode_attachment(input: &[u8], output: &mut impl Writer) -> usize {
    let (body, trailing) = match input {
        [head @ .., last @ (b' ' | b'\t')] => (head, Some(*last)),
        _ => (input, None),
    };
    let mut buffer = [0u8; ESCAPE_BUFFER_LEN];
    let mut column = 0;
    let mut rest = body;
    loop {
        let Some(stop) = find_attachment_escape(rest) else {
            column = write_plain(rest, output, column);
            break;
        };
        let (run, tail) = split_at_safe(rest, stop);
        column = write_plain(run, output, column);
        let (escapes, next) = split_at_safe(tail, attachment_escape_len(tail));
        column = write_escapes(escapes, output, column, &mut buffer);
        rest = next;
    }
    match trailing {
        Some(ch) => write_escape(ch, output, column),
        None => column,
    }
}

pub(crate) fn quoted_printable_encode(
    input: &[u8],
    output: &mut impl Writer,
    is_body: bool,
) -> usize {
    if is_body {
        encode_body(input, output)
    } else {
        encode_attachment(input, output)
    }
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QuotedPrintableEncoder {
    preserve_line_breaks: bool,
}

impl QuotedPrintableEncoder {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            preserve_line_breaks: false,
        }
    }

    #[inline(always)]
    pub fn preserve_line_breaks(mut self) -> Self {
        self.preserve_line_breaks = true;
        self
    }

    #[inline(always)]
    pub fn encode(&self, input: &[u8]) -> io::Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(input.len() * 2);
        self.encode_into(input, &mut buf);
        Ok(buf)
    }

    #[inline(always)]
    pub fn encode_to_writer(&self, input: &[u8], output: &mut impl Write) -> io::Result<usize> {
        let capacity = input.len().saturating_mul(3).clamp(64, 64 * 1024);
        let mut writer = IoWriter::with_capacity(capacity, output);
        let bytes_written = self.encode_into(input, &mut writer);
        writer.into_result().map(|_| bytes_written)
    }

    #[inline(always)]
    pub fn encode_into(&self, input: &[u8], output: &mut impl Writer) -> usize {
        quoted_printable_encode(input, output, self.preserve_line_breaks)
    }
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
        ] {
            let mut output = Vec::new();
            super::quoted_printable_encode(input.as_bytes(), &mut output, true);
            assert_eq!(
                std::str::from_utf8(&output).unwrap(),
                expected_result_body,
                "body"
            );

            let mut output = Vec::new();
            super::quoted_printable_encode(input.as_bytes(), &mut output, false);
            assert_eq!(
                std::str::from_utf8(&output).unwrap(),
                expected_result_attachment,
                "attachment"
            );

            let mut output = Vec::new();
            super::inline_quoted_printable_encode(input.as_bytes(), &mut output);
            assert_eq!(
                std::str::from_utf8(&output).unwrap(),
                expected_result_inline,
                "inline"
            );
        }
    }

    #[test]
    fn encode_quoted_printable_wraps_at_76_columns() {
        let input = " ".repeat(100);
        let expected = " ".repeat(75) + "=\r\n" + &" ".repeat(24) + "=20";

        let mut output = Vec::new();
        super::quoted_printable_encode(input.as_bytes(), &mut output, true);
        assert_eq!(std::str::from_utf8(&output).unwrap(), expected, "body");

        let mut output = Vec::new();
        super::quoted_printable_encode(input.as_bytes(), &mut output, false);
        assert_eq!(
            std::str::from_utf8(&output).unwrap(),
            expected,
            "attachment"
        );

        let mut output = Vec::new();
        super::inline_quoted_printable_encode(input.as_bytes(), &mut output);
        assert_eq!(
            std::str::from_utf8(&output).unwrap(),
            "_".repeat(100),
            "inline"
        );

        let input = "é".repeat(100);
        let mut output = Vec::new();
        super::quoted_printable_encode(input.as_bytes(), &mut output, true);
        for line in output.split(|&ch| ch == b'\n') {
            let line = match line {
                [head @ .., b'\r'] => head,
                _ => line,
            };
            assert!(line.len() <= 76, "line of {} bytes", line.len());
        }
        assert_eq!(output.len(), 8 * 75 + 7 * 3);
        assert!(output.starts_with(&"=C3=A9".repeat(12).into_bytes()));
        assert!(output.ends_with(&"=C3=A9".repeat(12).into_bytes()));
    }
}

#[cfg(test)]
pub(crate) mod harness {
    pub(crate) struct Rng(u64);

    impl Rng {
        pub(crate) fn new(seed: u64) -> Self {
            Rng(seed | 1)
        }

        pub(crate) fn next_u64(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.0 = x;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        pub(crate) fn below(&mut self, bound: usize) -> usize {
            (self.next_u64() % bound as u64) as usize
        }

        pub(crate) fn byte(&mut self) -> u8 {
            (self.next_u64() >> 33) as u8
        }
    }

    const TEXT: &[u8] =
        b"abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789.,;:!@#$%^&*()[]{}<>|/'\"-+";

    pub(crate) fn generate(rng: &mut Rng, kind: usize, len: usize) -> Vec<u8> {
        let mut out = Vec::with_capacity(len + 4);
        while out.len() < len {
            match kind {
                0 => out.push(TEXT[rng.below(TEXT.len())]),
                1 => match rng.below(24) {
                    0 | 1 => out.extend_from_slice(b"\r\n"),
                    2 => out.push(b'\n'),
                    3 => out.push(b'\r'),
                    4..=6 => out.push(b' '),
                    7 => out.push(b'\t'),
                    8 => out.push(b'='),
                    9 => out.push(b'?'),
                    10 => out.push(b'_'),
                    11 => out.push(rng.byte() | 0x80),
                    12 => out.push(0x7F),
                    _ => out.push(TEXT[rng.below(TEXT.len())]),
                },
                2 => match rng.below(16) {
                    0 => out.extend_from_slice(b" \r\n"),
                    1 => out.extend_from_slice(b"\t\n"),
                    2 => out.extend_from_slice("é".as_bytes()),
                    3 => out.extend_from_slice("世".as_bytes()),
                    4 => out.push(b' '),
                    _ => out.push(TEXT[rng.below(TEXT.len())]),
                },
                3 => out.push(rng.byte() | 0x80),
                4 => {
                    let word = 1 + rng.below(12);
                    for _ in 0..word {
                        out.push(TEXT[rng.below(TEXT.len())]);
                    }
                    out.push(b' ');
                }
                _ => out.push(rng.byte()),
            }
        }
        out.truncate(len);
        out
    }

    pub(crate) fn case_len(index: usize) -> usize {
        match index % 4 {
            0 => index % 64,
            1 => index % 211,
            2 => index % 787,
            _ => index % 3001,
        }
    }

    pub(crate) fn hex_value(ch: u8) -> Option<u8> {
        match ch {
            b'0'..=b'9' => Some(ch - b'0'),
            b'A'..=b'F' => Some(ch - b'A' + 10),
            _ => None,
        }
    }

    pub(crate) fn qp_decode_canonical(input: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(input.len());
        let mut rest = input;
        loop {
            match rest {
                [b'=', b'\r', b'\n', tail @ ..] | [b'=', b'\n', tail @ ..] => rest = tail,
                [b'=', high, low, tail @ ..]
                    if hex_value(*high).is_some() && hex_value(*low).is_some() =>
                {
                    let (Some(high), Some(low)) = (hex_value(*high), hex_value(*low)) else {
                        unreachable!()
                    };
                    out.push(high << 4 | low);
                    rest = tail;
                }
                [b'\r', b'\n', tail @ ..] | [b'\n', tail @ ..] => {
                    out.extend_from_slice(b"\r\n");
                    rest = tail;
                }
                [ch, tail @ ..] => {
                    out.push(*ch);
                    rest = tail;
                }
                [] => return out,
            }
        }
    }

    pub(crate) fn check_lines(encoded: &[u8], input: &[u8], kind: usize) {
        let mut rest = encoded;
        loop {
            let (line, tail) = match rest.windows(2).position(|pair| pair == b"\r\n") {
                Some(at) => {
                    let (line, tail) = rest.split_at(at);
                    (line, tail.get(2..).unwrap_or_default())
                }
                None => (rest, &[][..]),
            };
            assert!(
                line.len() <= 76,
                "line of {} bytes for kind {kind} input {:?}",
                line.len(),
                String::from_utf8_lossy(input)
            );
            assert!(
                !matches!(line.last(), Some(b' ' | b'\t')),
                "line ends with white space for kind {kind} input {:?}",
                String::from_utf8_lossy(input)
            );
            if tail.is_empty() {
                break;
            }
            rest = tail;
        }
        for (pos, &ch) in encoded.iter().enumerate() {
            if ch == b'\n' {
                assert_eq!(
                    encoded.get(pos.wrapping_sub(1)),
                    Some(&b'\r'),
                    "bare line feed at {pos} for kind {kind} input {:?}",
                    String::from_utf8_lossy(input)
                );
            }
        }
    }
}

#[cfg(test)]
mod properties {
    use super::harness::{Rng, case_len, check_lines, generate, hex_value, qp_decode_canonical};
    use super::{
        inline_quoted_printable_encode, phrase_quoted_printable_encode, quoted_printable_encode,
    };

    const CASES: usize = 4_000;
    const KINDS: usize = 6;

    fn expected_body(input: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(input.len() + 16);
        let mut previous = 0;
        for &byte in input {
            if byte == b'\n' && previous != b'\r' {
                out.push(b'\r');
            }
            out.push(byte);
            previous = byte;
        }
        out
    }

    fn check_quoted_printable(input: &[u8], is_body: bool, kind: usize) {
        let mut encoded = Vec::new();
        quoted_printable_encode(input, &mut encoded, is_body);
        check_lines(&encoded, input, kind);
        let expected = if is_body {
            expected_body(input)
        } else {
            input.to_vec()
        };
        assert_eq!(
            qp_decode_canonical(&encoded),
            expected,
            "kind {kind} is_body {is_body} input {:?}",
            String::from_utf8_lossy(input)
        );
    }

    fn q_decode(encoded: &[u8]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut rest = encoded;
        loop {
            match rest {
                [b'=', high, low, tail @ ..] => {
                    let (Some(high), Some(low)) = (hex_value(*high), hex_value(*low)) else {
                        panic!("bad escape in {:?}", String::from_utf8_lossy(encoded));
                    };
                    out.push(high << 4 | low);
                    rest = tail;
                }
                [b'_', tail @ ..] => {
                    out.push(b' ');
                    rest = tail;
                }
                [ch, tail @ ..] => {
                    out.push(*ch);
                    rest = tail;
                }
                [] => return out,
            }
        }
    }

    #[test]
    fn body_and_attachment_modes_decode_to_the_input() {
        let mut rng = Rng::new(0xB0D4_0001);
        for index in 0..CASES {
            let kind = index % KINDS;
            let input = generate(&mut rng, kind, case_len(index));
            check_quoted_printable(&input, true, kind);
            check_quoted_printable(&input, false, kind);
        }
    }

    #[test]
    fn every_length_around_the_line_limit_decodes_to_the_input() {
        for kind in 0..KINDS {
            let mut rng = Rng::new(0x11FE_0005 + kind as u64);
            for len in 0..400 {
                let input = generate(&mut rng, kind, len);
                check_quoted_printable(&input, true, kind);
                check_quoted_printable(&input, false, kind);
            }
        }
    }

    #[test]
    fn large_inputs_decode_to_the_input() {
        let mut rng = Rng::new(0x7A17_0004);
        for kind in 0..KINDS {
            let input = generate(&mut rng, kind, 1 << 20);
            check_quoted_printable(&input, true, kind);
            check_quoted_printable(&input, false, kind);
        }
    }

    #[test]
    fn q_encodings_round_trip_and_use_only_safe_characters() {
        let mut rng = Rng::new(0x1471_0003);
        for index in 0..CASES {
            let kind = index % KINDS;
            let input = generate(&mut rng, kind, case_len(index) % 300);

            let mut inline = Vec::new();
            let len = inline_quoted_printable_encode(&input, &mut inline);
            assert_eq!(len, inline.len(), "inline length kind {kind}");
            assert_eq!(q_decode(&inline), input, "inline kind {kind}");
            assert!(
                inline
                    .iter()
                    .all(|&ch| ch < 127 && !matches!(ch, b'?' | b' ' | b'\t' | b'\r' | b'\n')),
                "inline characters kind {kind}: {:?}",
                String::from_utf8_lossy(&inline)
            );

            let mut phrase = Vec::new();
            let len = phrase_quoted_printable_encode(&input, &mut phrase);
            assert_eq!(len, phrase.len(), "phrase length kind {kind}");
            assert_eq!(q_decode(&phrase), input, "phrase kind {kind}");
            let mut rest = phrase.as_slice();
            while let Some((&ch, tail)) = rest.split_first() {
                if ch == b'=' {
                    rest = tail.get(2..).unwrap_or_default();
                    continue;
                }
                assert!(
                    ch.is_ascii_alphanumeric()
                        || matches!(ch, b'!' | b'*' | b'+' | b'-' | b'/' | b'_'),
                    "phrase character {ch} kind {kind}"
                );
                rest = tail;
            }
        }
    }

    #[test]
    fn byte_length_tables_match_the_byte_encoders() {
        for value in 0..=u8::MAX {
            let mut out = Vec::new();
            let len = super::quoted_printable_encode_byte(value, &mut out);
            assert_eq!(len, out.len(), "byte {value}");
            assert_eq!(len, super::quoted_printable_byte_len(value), "len {value}");

            let mut out = Vec::new();
            let len = super::quoted_printable_encode_phrase_byte(value, &mut out);
            assert_eq!(len, out.len(), "phrase byte {value}");
            assert_eq!(
                len,
                super::quoted_printable_phrase_byte_len(value),
                "phrase len {value}"
            );
        }
    }
}
