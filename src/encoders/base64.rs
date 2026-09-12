/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use crate::writer::{IoWriter, Writer};
use std::io::{self, Write};

const CHARPAD: u8 = b'=';
const CRLF: [u8; 2] = *b"\r\n";
const LINE_GROUPS: usize = 19;
const LINE_INPUT: usize = LINE_GROUPS * 3;
const LINE_OUTPUT: usize = LINE_GROUPS * 4;
const LINE_TOTAL: usize = LINE_OUTPUT + 2;
const INLINE_BLOCK_INPUT: usize = 12288;
const INLINE_BLOCK_OUTPUT: usize = INLINE_BLOCK_INPUT / 3 * 4;
const WRAPPED_BLOCK_LINES: usize = 52;
const WRAPPED_BLOCK_INPUT: usize = WRAPPED_BLOCK_LINES * LINE_INPUT;
const WRAPPED_BLOCK_OUTPUT: usize = WRAPPED_BLOCK_LINES * LINE_TOTAL;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const PAIRS: [u16; 4096] = {
    let mut table = [0u16; 4096];
    let mut index = 0;
    while index < table.len() {
        table[index] = u16::from_ne_bytes([ALPHABET[index >> 6], ALPHABET[index & 0x3f]]);
        index += 1;
    }
    table
};

/// Number of bytes `base64_encode_slice` writes for `input_len` input bytes.
pub const fn base64_encoded_len(input_len: usize) -> usize {
    input_len.div_ceil(3) * 4
}

#[inline(always)]
fn encode_pair(index: usize) -> u32 {
    let high = PAIRS[(index >> 12) & 0xfff] as u32;
    let low = PAIRS[index & 0xfff] as u32;
    if cfg!(target_endian = "little") {
        high | (low << 16)
    } else {
        (high << 16) | low
    }
}

#[inline(always)]
fn encode_group(group: [u8; 3]) -> [u8; 4] {
    let [b0, b1, b2] = group;
    encode_pair(((b0 as usize) << 16) | ((b1 as usize) << 8) | b2 as usize).to_ne_bytes()
}

#[inline(always)]
fn encode_block(block: &[u8; 12], slot: &mut [u8; 16]) {
    let (Some(head), Some(foot)) = (block.first_chunk::<8>(), block.last_chunk::<8>()) else {
        return;
    };
    let first = u64::from_be_bytes(*head);
    let second = u64::from_be_bytes(*foot);
    if let [a, b, c, d] = slot.as_chunks_mut::<4>().0 {
        *a = encode_pair((first >> 40) as usize).to_ne_bytes();
        *b = encode_pair((first >> 16) as usize & 0xff_ffff).to_ne_bytes();
        *c = encode_pair((second >> 24) as usize & 0xff_ffff).to_ne_bytes();
        *d = encode_pair(second as usize & 0xff_ffff).to_ne_bytes();
    }
}

#[inline(always)]
fn encode_nine(nine: &[u8; 9], slot: &mut [u8; 12]) {
    let (Some(head), Some(&last)) = (nine.first_chunk::<8>(), nine.last()) else {
        return;
    };
    let word = u64::from_be_bytes(*head);
    if let [a, b, c] = slot.as_chunks_mut::<4>().0 {
        *a = encode_pair((word >> 40) as usize).to_ne_bytes();
        *b = encode_pair((word >> 16) as usize & 0xff_ffff).to_ne_bytes();
        *c = encode_pair(((word as usize & 0xffff) << 8) | last as usize).to_ne_bytes();
    }
}

#[inline(always)]
fn encode_line(line: &[u8; LINE_INPUT], slot: &mut [u8; LINE_TOTAL]) {
    let (blocks, rest) = line.as_chunks::<12>();
    let (Some(nine), Some((head, tail))) =
        (rest.first_chunk::<9>(), slot.split_first_chunk_mut::<64>())
    else {
        return;
    };
    for (block, slot) in blocks.iter().zip(head.as_chunks_mut::<16>().0.iter_mut()) {
        encode_block(block, slot);
    }
    let Some((twelve, crlf)) = tail.split_first_chunk_mut::<12>() else {
        return;
    };
    encode_nine(nine, twelve);
    if let Some(crlf) = crlf.first_chunk_mut::<2>() {
        *crlf = CRLF;
    }
}

#[inline(always)]
fn encode_tail(tail: &[u8]) -> [u8; 4] {
    match *tail {
        [b0] => {
            let [c0, c1, _, _] = encode_group([b0, 0, 0]);
            [c0, c1, CHARPAD, CHARPAD]
        }
        [b0, b1] => {
            let [c0, c1, c2, _] = encode_group([b0, b1, 0]);
            [c0, c1, c2, CHARPAD]
        }
        _ => [CHARPAD; 4],
    }
}

#[inline(always)]
fn encode_exact(input: &[u8], output: &mut [u8]) {
    let (blocks, rest) = input.as_chunks::<12>();
    let Some((slots, rest_slots)) = output.split_at_mut_checked(blocks.len() * 16) else {
        return;
    };
    for (block, slot) in blocks.iter().zip(slots.as_chunks_mut::<16>().0.iter_mut()) {
        encode_block(block, slot);
    }

    let (groups, tail) = rest.as_chunks::<3>();
    let Some((group_slots, tail_slot)) = rest_slots.split_at_mut_checked(groups.len() * 4) else {
        return;
    };
    for (group, slot) in groups
        .iter()
        .zip(group_slots.as_chunks_mut::<4>().0.iter_mut())
    {
        *slot = encode_group(*group);
    }
    if !tail.is_empty()
        && let Some(slot) = tail_slot.first_chunk_mut::<4>()
    {
        *slot = encode_tail(tail);
    }
}

#[inline(never)]
fn encode_truncated(input: &[u8], output: &mut [u8]) -> usize {
    let groups = (input.len() / 3).min(output.len() / 4);
    let Some((body, tail)) = input.split_at_checked(groups * 3) else {
        return 0;
    };
    let Some((slots, spare)) = output.split_at_mut_checked(groups * 4) else {
        return 0;
    };

    encode_exact(body, slots);
    let mut written = groups * 4;
    if !tail.is_empty()
        && let Some(slot) = spare.first_chunk_mut::<4>()
    {
        *slot = encode_tail(tail);
        written += 4;
    }

    written
}

/// Encodes `input` as standard base64 (RFC 4648, `+` and `/`, `=` padding,
/// no line breaks) into `output` and returns the number of bytes written.
///
/// `output` must hold at least [`base64_encoded_len`] bytes; otherwise only
/// the complete 4-byte groups that fit are written.
#[inline]
pub fn base64_encode_slice(input: &[u8], output: &mut [u8]) -> usize {
    let written = base64_encoded_len(input.len());
    match output.get_mut(..written) {
        Some(slots) => {
            encode_exact(input, slots);
            written
        }
        None => encode_truncated(input, output),
    }
}

pub(crate) fn base64_encode_inline(input: &[u8], output: &mut impl Writer) -> usize {
    let (blocks, rest) = input.as_chunks::<INLINE_BLOCK_INPUT>();
    let mut written = 0;

    for block in blocks {
        output.write_with(INLINE_BLOCK_OUTPUT, |region| {
            base64_encode_slice(block, region)
        });
        written += INLINE_BLOCK_OUTPUT;
    }

    if !rest.is_empty() {
        let len = base64_encoded_len(rest.len());
        output.write_with(len, |region| base64_encode_slice(rest, region));
        written += len;
    }

    written
}

#[inline(always)]
fn fill_lines(input: &[u8], region: &mut [u8]) -> usize {
    let (lines, rest) = input.as_chunks::<LINE_INPUT>();
    let Some((slots, spare)) = region.split_at_mut_checked(lines.len() * LINE_TOTAL) else {
        return 0;
    };
    let mut written = 0;

    for (line, slot) in lines
        .iter()
        .zip(slots.as_chunks_mut::<LINE_TOTAL>().0.iter_mut())
    {
        encode_line(line, slot);
        written += LINE_TOTAL;
    }

    if !rest.is_empty() {
        let encoded = base64_encode_slice(rest, spare);
        written += encoded;
        if let Some((_, after)) = spare.split_at_mut_checked(encoded)
            && let Some(crlf) = after.first_chunk_mut::<2>()
        {
            *crlf = CRLF;
            written += 2;
        }
    }

    written
}

pub(crate) fn base64_encode_wrapped(input: &[u8], output: &mut impl Writer) -> usize {
    let (blocks, rest) = input.as_chunks::<WRAPPED_BLOCK_INPUT>();
    let mut written = 0;

    for block in blocks {
        output.write_with(WRAPPED_BLOCK_OUTPUT, |region| fill_lines(block, region));
        written += WRAPPED_BLOCK_LINES * LINE_OUTPUT;
    }

    if !rest.is_empty() {
        let encoded = base64_encoded_len(rest.len());
        let len = encoded + rest.len().div_ceil(LINE_INPUT) * 2;
        output.write_with(len, |region| fill_lines(rest, region));
        written += encoded;
    }

    written
}

#[repr(transparent)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Base64Encoder {
    wrap_lines: bool,
}

impl Base64Encoder {
    #[inline(always)]
    pub fn new() -> Self {
        Self { wrap_lines: false }
    }

    #[inline(always)]
    pub fn wrap_lines(mut self) -> Self {
        self.wrap_lines = true;
        self
    }

    #[inline(always)]
    pub fn encode(&self, input: &[u8]) -> io::Result<Vec<u8>> {
        let encoded = base64_encoded_len(input.len());
        let capacity = if self.wrap_lines {
            encoded + encoded.div_ceil(LINE_OUTPUT) * 2
        } else {
            encoded
        };
        let mut buf = Vec::with_capacity(capacity);
        self.encode_into(input, &mut buf);
        Ok(buf)
    }

    #[inline(always)]
    pub fn encode_to_writer(&self, input: &[u8], output: &mut impl Write) -> io::Result<usize> {
        let capacity = base64_encoded_len(input.len())
            .saturating_add(input.len().div_ceil(LINE_INPUT).saturating_mul(2))
            .clamp(64, 64 * 1024);
        let mut writer = IoWriter::with_capacity(capacity, output);
        let bytes_written = self.encode_into(input, &mut writer);
        writer.into_result().map(|_| bytes_written)
    }

    #[inline(always)]
    pub fn encode_into(&self, input: &[u8], output: &mut impl Writer) -> usize {
        if self.wrap_lines {
            base64_encode_wrapped(input, output)
        } else {
            base64_encode_inline(input, output)
        }
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {
    use super::*;

    #[test]
    fn encode_base64() {
        for (input, expected_result) in [
            ("Test".to_string(), "VGVzdA==\r\n"),
            ("Ye".to_string(), "WWU=\r\n"),
            ("A".to_string(), "QQ==\r\n"),
            ("ro".to_string(), "cm8=\r\n"),
            (
                "Are you a Shimano or Campagnolo person?".to_string(),
                "QXJlIHlvdSBhIFNoaW1hbm8gb3IgQ2FtcGFnbm9sbyBwZXJzb24/\r\n",
            ),
            (
                "<!DOCTYPE html>\n<html>\n<body>\n</body>\n</html>\n".to_string(),
                "PCFET0NUWVBFIGh0bWw+CjxodG1sPgo8Ym9keT4KPC9ib2R5Pgo8L2h0bWw+Cg==\r\n",
            ),
            ("áéíóú".to_string(), "w6HDqcOtw7PDug==\r\n"),
            (
                " ".repeat(100),
                concat!(
                    "ICAgICAgICAgICAgICAgICAgICAgICAgICAgICAg",
                    "ICAgICAgICAgICAgICAgICAgICAgICAgICAg\r\n",
                    "ICAgICAgICAgICAgICAgICAgICAgICAgICAgICAg",
                    "ICAgICAgICAgICAgIA==\r\n",
                ),
            ),
        ] {
            let mut output = Vec::new();
            base64_encode_wrapped(input.as_bytes(), &mut output);
            assert_eq!(std::str::from_utf8(&output).unwrap(), expected_result);

            let mut inline = Vec::new();
            base64_encode_inline(input.as_bytes(), &mut inline);
            assert_eq!(
                std::str::from_utf8(&inline).unwrap(),
                expected_result.replace("\r\n", "")
            );

            let mut slice = vec![0u8; base64_encoded_len(input.len())];
            let written = base64_encode_slice(input.as_bytes(), &mut slice);
            assert_eq!(written, slice.len());
            assert_eq!(slice, inline);
        }
    }

    fn reference_encode(input: &[u8]) -> Vec<u8> {
        let alphabet = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        for group in input.chunks(3) {
            let word = group.iter().enumerate().fold(0u32, |word, (pos, &byte)| {
                word | (byte as u32) << (16 - pos * 8)
            });
            let chars = [
                alphabet[(word >> 18) as usize & 63],
                alphabet[(word >> 12) as usize & 63],
                alphabet[(word >> 6) as usize & 63],
                alphabet[word as usize & 63],
            ];
            out.extend_from_slice(&chars[..group.len() + 1]);
            out.extend(std::iter::repeat_n(b'=', 3 - group.len()));
        }
        out
    }

    #[test]
    fn every_length_matches_a_reference_encoder() {
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut input = Vec::new();
        for len in 0..=600usize {
            input.clear();
            while input.len() < len {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                input.extend_from_slice(&state.to_le_bytes());
            }
            input.truncate(len);
            let expected = reference_encode(&input);

            let mut slice = vec![0xAAu8; base64_encoded_len(len) + 8];
            let written = base64_encode_slice(&input, &mut slice);
            assert_eq!(written, expected.len(), "length {len}");
            assert_eq!(&slice[..written], expected.as_slice(), "length {len}");
            assert!(
                slice[written..].iter().all(|&byte| byte == 0xAA),
                "length {len}"
            );

            let mut inline = Vec::new();
            assert_eq!(base64_encode_inline(&input, &mut inline), expected.len());
            assert_eq!(inline, expected, "inline {len}");

            let mut windowed = Vec::new();
            for window in input.chunks(192) {
                let mut buffer = [0u8; 256];
                let written = base64_encode_slice(window, &mut buffer);
                windowed.extend_from_slice(&buffer[..written]);
            }
            assert_eq!(windowed, expected, "windowed {len}");

            let mut wrapped = Vec::new();
            base64_encode_wrapped(&input, &mut wrapped);
            let joined: Vec<u8> = wrapped
                .iter()
                .copied()
                .filter(|&b| b != b'\r' && b != b'\n')
                .collect();
            assert_eq!(joined, expected, "wrapped {len}");
            for line in wrapped.split(|&b| b == b'\n') {
                assert!(line.len() <= 77, "wrapped line {len}");
            }
            if len > 0 {
                assert!(wrapped.ends_with(b"\r\n"), "wrapped terminator {len}");
            }

            let encoded = Base64Encoder::new().wrap_lines().encode(&input).unwrap();
            assert_eq!(encoded, wrapped);
            assert_eq!(encoded.capacity(), encoded.len(), "exact capacity {len}");
        }
    }

    #[test]
    fn slice_encoding_truncates_when_output_is_short() {
        let mut output = [0u8; 6];
        assert_eq!(base64_encode_slice(b"abcdef", &mut output), 4);
        assert_eq!(&output[..4], b"YWJj");
        assert_eq!(base64_encode_slice(b"abcd", &mut output), 4);
    }
}

/*
 * Table adapted from Nick Galbreath's "High performance base64 encoder / decoder"
 *
 * Copyright 2005, 2006, 2007 Nick Galbreath -- nickg [at] modp [dot] com
 * All rights reserved.
 *
 * http://code.google.com/p/stringencoders/
 *
 * Released under bsd license.
 *
 */

pub static E0: &[u8] = b"AAAABBBBCCCCDDDDEEEEFFFFGGGGHHHHIIIIJJJJKKKKLLLLMMMMNNNNOOOOPPPPQQQQRRRRSSSSTTTTUUUUVVVVWWWWXXXXYYYYZZZZaaaabbbbccccddddeeeeffffgggghhhhiiiijjjjkkkkllllmmmmnnnnooooppppqqqqrrrrssssttttuuuuvvvvwwwwxxxxyyyyzzzz0000111122223333444455556666777788889999++++////";
pub static E1: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
pub static E2: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
