/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use super::fold::{FoldWriter, zero_lane};
use crate::{
    encoders::{
        encode::EncodingType,
        swar::{swar_del_or_above, swar_zero_lanes},
    },
    writer::Writer,
};
use encodify::hex::PERCENT;

const EXTENDED_CHARSET: &[u8] = b"UTF-8''";
const MAX_PARAMETER_ATOM: usize = 75;
const MIN_SECTION_PAYLOAD: usize = 12;

const fn escape_table() -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut byte = 0;

    while byte < 256 {
        table[byte] = matches!(byte as u8, b'\\' | b'"' | b'\r' | b'\n') as u8;
        byte += 1;
    }

    table
}

const ESCAPE: [u8; 256] = escape_table();

#[inline(always)]
fn needs_escape(byte: u8) -> bool {
    ESCAPE[byte as usize] != 0
}

#[inline(always)]
const fn is_wsp(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t')
}

#[inline(always)]
const fn quoted_byte_len(byte: u8) -> usize {
    match byte {
        b'\\' | b'"' => 2,
        b'\r' | b'\n' => 0,
        _ => 1,
    }
}

#[inline(always)]
fn has_escape(bytes: &[u8]) -> bool {
    const BACKSLASH: u64 = u64::from_ne_bytes([b'\\'; 8]);
    const QUOTE: u64 = u64::from_ne_bytes([b'"'; 8]);
    const CR: u64 = u64::from_ne_bytes([b'\r'; 8]);
    const LF: u64 = u64::from_ne_bytes([b'\n'; 8]);

    let (chunks, tail) = bytes.as_chunks::<8>();
    let mut found = 0;

    for chunk in chunks {
        let word = u64::from_ne_bytes(*chunk);
        found |= zero_lane(word ^ BACKSLASH)
            | zero_lane(word ^ QUOTE)
            | zero_lane(word ^ CR)
            | zero_lane(word ^ LF);
    }

    if !tail.is_empty() {
        match bytes.last_chunk::<8>() {
            Some(last) => {
                let word = u64::from_ne_bytes(*last);
                found |= zero_lane(word ^ BACKSLASH)
                    | zero_lane(word ^ QUOTE)
                    | zero_lane(word ^ CR)
                    | zero_lane(word ^ LF);
            }
            None => return found != 0 || tail.iter().any(|&byte| needs_escape(byte)),
        }
    }

    found != 0
}

#[inline(always)]
fn quoted_metrics(bytes: &[u8]) -> (usize, bool) {
    if has_escape(bytes) {
        (
            bytes
                .iter()
                .fold(0, |len, &byte| len + quoted_byte_len(byte)),
            false,
        )
    } else {
        (bytes.len(), true)
    }
}

#[inline(always)]
pub(crate) fn quoted_string_metrics(input: &str) -> (usize, bool) {
    let (len, verbatim) = quoted_metrics(input.as_bytes());
    (len + 2, verbatim)
}

#[inline(always)]
fn write_quoted_known<W: Writer>(folder: &mut FoldWriter<'_, W>, bytes: &[u8], verbatim: bool) {
    if verbatim {
        folder.write_short(bytes);
    } else {
        write_quoted_bytes(folder, bytes);
    }
}

fn write_quoted_bytes<W: Writer>(folder: &mut FoldWriter<'_, W>, bytes: &[u8]) {
    let mut rest = bytes;
    while let Some(pos) = rest.iter().position(|&byte| needs_escape(byte)) {
        let (head, tail) = rest.split_at(pos);
        folder.write_short(head);
        match tail {
            [b'\\', ..] => folder.write(b"\\\\"),
            [b'"', ..] => folder.write(b"\\\""),
            _ => (),
        }
        rest = tail.get(1..).unwrap_or_default();
    }
    folder.write_short(rest);
}

fn write_quoted_words<W: Writer>(folder: &mut FoldWriter<'_, W>, bytes: &[u8], close: usize) {
    let mut rest = bytes;

    loop {
        let ws_len = rest
            .iter()
            .position(|&byte| !is_wsp(byte))
            .unwrap_or(rest.len());
        let (ws, after) = rest.split_at(ws_len);

        if after.is_empty() {
            folder.write(ws);
            return;
        }

        let word_len = after
            .iter()
            .position(|&byte| is_wsp(byte))
            .unwrap_or(after.len());
        let (word, next) = after.split_at(word_len);
        let (len, verbatim) = quoted_metrics(word);
        let closing = if next.is_empty() { close + 1 } else { 0 };

        folder.keep_ws(ws, len + closing);
        write_quoted_known(folder, word, verbatim);
        rest = next;
    }
}

fn write_quoted_inner<W: Writer>(
    folder: &mut FoldWriter<'_, W>,
    input: &str,
    len: usize,
    verbatim: bool,
    close: usize,
) {
    if folder.fits(len - 1 + close) {
        write_quoted_known(folder, input.as_bytes(), verbatim);
    } else {
        write_quoted_words(folder, input.as_bytes(), close);
    }
}

fn write_quoted_body<W: Writer>(
    folder: &mut FoldWriter<'_, W>,
    input: &str,
    len: usize,
    verbatim: bool,
    close: usize,
) {
    folder.write_byte(b'"');
    write_quoted_inner(folder, input, len, verbatim, close);
    folder.write_byte(b'"');
}

pub(crate) fn write_quoted_string<W: Writer>(
    folder: &mut FoldWriter<'_, W>,
    input: &str,
    tail: &[u8],
) {
    let upper = input.len() * 2 + 2 + tail.len();

    if folder.certainly_fits(upper) {
        folder.begin_atom(upper);
        folder.write_byte(b'"');
        write_quoted_known(folder, input.as_bytes(), !has_escape(input.as_bytes()));
        folder.write_byte(b'"');
    } else {
        let (len, verbatim) = quoted_string_metrics(input);
        folder.begin_atom(len + tail.len());
        write_quoted_body(folder, input, len, verbatim, tail.len());
    }

    folder.write_tail(tail);
}

/// Writes an RFC 5322 phrase: a quoted string, or encoded words when the
/// value cannot be represented as US-ASCII.
#[inline]
pub(crate) fn write_phrase<W: Writer>(folder: &mut FoldWriter<'_, W>, name: &str, tail: &[u8]) {
    match EncodingType::for_header(name.as_bytes()) {
        EncodingType::Base64 => folder.write_b_words(name, tail),
        EncodingType::QuotedPrintable(is_ascii) => {
            folder.write_q_words::<true>(name, is_ascii, tail)
        }
        EncodingType::None => write_quoted_string(folder, name, tail),
    }
}

/// Writes `key=value`, quoting the value when it needs it and folding it,
/// when the value does not fit, inside its quoted string. Values that carry
/// non-ASCII or control characters are written as RFC 2231 extended
/// parameters instead. `reserve` is the room a following separator needs on
/// the same line.
#[inline]
pub(crate) fn write_parameter<W: Writer>(
    folder: &mut FoldWriter<'_, W>,
    key: &str,
    value: &str,
    reserve: usize,
) {
    if needs_extended_encoding(value.as_bytes()) {
        return write_extended_parameter(folder, key, value, reserve);
    }

    let fixed = key.len() + 1 + reserve;
    let upper = fixed + value.len() * 2 + 2;

    if folder.certainly_fits(upper) {
        folder.begin_atom(upper);
        folder.write(key.as_bytes());
        folder.write(b"=\"");
        write_quoted_known(folder, value.as_bytes(), !has_escape(value.as_bytes()));
    } else {
        let (len, verbatim) = quoted_string_metrics(value);
        folder.begin_atom(fixed + len);
        folder.write(key.as_bytes());
        folder.write(b"=\"");
        write_quoted_inner(folder, value, len, verbatim, reserve);
    }

    folder.write_byte(b'"');
}

#[inline]
fn needs_extended_encoding(bytes: &[u8]) -> bool {
    const HIGH_BITS: u64 = 0xE0E0_E0E0_E0E0_E0E0;

    let (chunks, tail) = bytes.as_chunks::<8>();
    let mut found = 0;

    for chunk in chunks {
        let word = u64::from_ne_bytes(*chunk);
        found |= swar_del_or_above(word) | swar_zero_lanes(word & HIGH_BITS);
    }

    found != 0 || tail.iter().any(|&byte| !(0x20..0x7F).contains(&byte))
}

fn section_len(text: &str, budget: usize) -> (usize, usize) {
    let mut taken = 0;
    let mut len = 0;

    for (start, ch) in text.char_indices() {
        let end = start + ch.len_utf8();
        let char_len = PERCENT.encoded_len(text.as_bytes().get(start..end).unwrap_or_default());
        if len + char_len > budget && taken > 0 {
            break;
        }
        len += char_len;
        taken = end;
    }

    (taken, len)
}

fn write_section_prefix<W: Writer>(folder: &mut FoldWriter<'_, W>, key: &str, section: usize) {
    let mut digits = [0u8; 20];
    let mut at = digits.len();
    let mut value = section;
    loop {
        at -= 1;
        if let Some(slot) = digits.get_mut(at) {
            *slot = b'0' + (value % 10) as u8;
        }
        value /= 10;
        if value == 0 || at == 0 {
            break;
        }
    }

    folder.write(key.as_bytes());
    folder.write_byte(b'*');
    folder.write(digits.get(at..).unwrap_or_default());
    folder.write(b"*=");
    if section == 0 {
        folder.write(EXTENDED_CHARSET);
    }
}

fn write_extended_parameter<W: Writer>(
    folder: &mut FoldWriter<'_, W>,
    key: &str,
    value: &str,
    reserve: usize,
) {
    let prefix = key.len() + 2 + EXTENDED_CHARSET.len();
    let budget = MAX_PARAMETER_ATOM.saturating_sub(prefix + reserve);
    let whole = (value.len() <= budget)
        .then(|| PERCENT.encoded_len(value))
        .filter(|&len| len <= budget);

    if let Some(len) = whole {
        folder.begin_atom(prefix + len + reserve);
        folder.write(key.as_bytes());
        folder.write(b"*=");
        folder.write(EXTENDED_CHARSET);
        folder.append_with(len, |buffer| PERCENT.encode_append(value, buffer));
        return;
    }

    let mut rest = value;
    let mut section: usize = 0;

    loop {
        let digits = section.checked_ilog10().map_or(1, |log| log as usize + 1);
        let charset = if section == 0 {
            EXTENDED_CHARSET.len()
        } else {
            0
        };
        let prefix = key.len() + 3 + digits + charset;
        let budget = MAX_PARAMETER_ATOM
            .saturating_sub(prefix + 1)
            .max(MIN_SECTION_PAYLOAD);
        let (taken, len) = section_len(rest, budget);
        let (chunk, tail) = rest.split_at_checked(taken).unwrap_or((rest, ""));

        if section > 0 {
            folder.semicolon();
        }
        folder.begin_atom(prefix + len + if tail.is_empty() { reserve } else { 1 });
        write_section_prefix(folder, key, section);
        folder.append_with(len, |buffer| PERCENT.encode_append(chunk, buffer));

        if tail.is_empty() {
            return;
        }
        rest = tail;
        section += 1;
    }
}
