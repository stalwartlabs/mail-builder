/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use crate::{
    encoders::{
        base64::{base64_encode_inline, base64_encoded_len},
        quoted_printable::{
            inline_quoted_printable_encode, phrase_quoted_printable_encode,
            quoted_printable_byte_len, quoted_printable_encode_byte,
            quoted_printable_encode_phrase_byte, quoted_printable_phrase_byte_len,
        },
    },
    writer::Writer,
};

/// Line length RFC 5322 section 2.1.1 recommends staying below.
pub(crate) const FOLD_TARGET: usize = 78;

pub(crate) const MAX_ENCODED_WORD: usize = 75;
const Q_UTF8: &[u8] = b"=?utf-8?Q?";
const Q_ASCII: &[u8] = b"=?us-ascii?Q?";
pub(crate) const B_UTF8: &[u8] = b"=?utf-8?B?";
pub(crate) const B_OVERHEAD: usize = B_UTF8.len() + 2;
const MAX_Q_CHAR_LEN: usize = 12;
const MAX_B_CHAR_LEN: usize = 8;
const SHORT_RUN_LEN: usize = 8;
const SHORT_WRITE_LEN: usize = 8;

#[inline(always)]
const fn is_break(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\r' | b'\n')
}

#[inline(always)]
const fn is_line_break(byte: u8) -> bool {
    matches!(byte, b'\r' | b'\n')
}

/// Header folder: the caller emits unbreakable atoms and foldable
/// whitespace, the folder decides where the line breaks.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pending {
    None,
    Space,
    Semicolon,
}

pub(crate) struct FoldWriter<'x, W: Writer> {
    output: &'x mut W,
    column: usize,
    line_start: usize,
    pending: Pending,
}

impl<'x, W: Writer> FoldWriter<'x, W> {
    #[inline(always)]
    pub(crate) fn new(output: &'x mut W, column: usize) -> Self {
        let folds_first = column >= FOLD_TARGET;
        FoldWriter {
            output,
            column,
            line_start: if folds_first { 0 } else { column },
            pending: if folds_first {
                Pending::Space
            } else {
                Pending::None
            },
        }
    }

    #[inline(always)]
    fn can_fold(&self) -> bool {
        self.column > self.line_start
    }

    #[inline(always)]
    pub(crate) fn write(&mut self, bytes: &[u8]) {
        self.output.write(bytes);
        self.column += bytes.len();
    }

    #[inline(always)]
    pub(crate) fn write_short(&mut self, bytes: &[u8]) {
        if bytes.len() < SHORT_WRITE_LEN {
            self.column += bytes.len();
            for &byte in bytes {
                self.output.write_byte(byte);
            }
        } else {
            self.write(bytes);
        }
    }

    #[inline(always)]
    pub(crate) fn write_tail(&mut self, tail: &[u8]) {
        match tail {
            [] => (),
            [byte] => self.write_byte(*byte),
            _ => self.write(tail),
        }
    }

    #[inline(always)]
    pub(crate) fn write_byte(&mut self, byte: u8) {
        self.output.write_byte(byte);
        self.column += 1;
    }

    #[inline(always)]
    pub(crate) fn write_encoded(&mut self, fill: impl FnOnce(&mut W) -> usize) {
        self.column += fill(self.output);
    }

    #[inline(always)]
    pub(crate) fn space(&mut self) {
        self.pending = Pending::Space;
    }

    #[inline(always)]
    pub(crate) fn semicolon(&mut self) {
        self.pending = Pending::Semicolon;
    }

    #[inline(always)]
    fn separator_len(&self) -> usize {
        match self.pending {
            Pending::None => 0,
            Pending::Space => 1,
            Pending::Semicolon => 2,
        }
    }

    #[inline(always)]
    pub(crate) fn begin_atom(&mut self, len: usize) {
        let pending = self.pending;
        if pending == Pending::None {
            return;
        }

        self.pending = Pending::None;
        let separator = usize::from(pending == Pending::Semicolon) + 1;

        if self.can_fold() && self.column + separator + len > FOLD_TARGET {
            match pending {
                Pending::Semicolon => self.output.write(b";\r\n "),
                _ => self.output.write(b"\r\n "),
            }
            self.column = 1;
            self.line_start = 1;
        } else {
            match pending {
                Pending::Semicolon => self.output.write(b"; "),
                _ => self.output.write_byte(b' '),
            }
            self.column += separator;
        }
    }

    #[inline(always)]
    fn fold_or_write(&mut self, ws: &[u8], atom_len: usize) {
        if self.can_fold() && self.column + ws.len() + atom_len > FOLD_TARGET {
            match ws {
                [b' '] => self.output.write(b"\r\n "),
                _ => {
                    self.output.write(b"\r\n");
                    self.output.write(ws);
                }
            }
            self.column = ws.len();
            self.line_start = ws.len();
        } else {
            self.write(ws);
        }
    }

    #[inline(always)]
    pub(crate) fn keep_ws(&mut self, ws: &[u8], atom_len: usize) {
        match ws {
            [] => (),
            [byte] if !is_line_break(*byte) => self.fold_or_write(ws, atom_len),
            _ => self.keep_mixed_ws(ws, atom_len),
        }
    }

    #[inline(never)]
    fn keep_mixed_ws(&mut self, ws: &[u8], atom_len: usize) {
        match ws.iter().rposition(|&byte| is_line_break(byte)) {
            Some(pos) => match ws.get(pos + 1..) {
                Some(indent) if !indent.is_empty() && self.can_fold() => {
                    self.output.write(b"\r\n");
                    self.output.write(indent);
                    self.column = indent.len();
                    self.line_start = indent.len();
                }
                Some(indent) if !indent.is_empty() => self.write(indent),
                _ => self.fold_or_write(b" ", atom_len),
            },
            None => self.fold_or_write(ws, atom_len),
        }
    }

    fn trailing_ws(&mut self, ws: &[u8]) {
        match ws.iter().rposition(|&byte| is_line_break(byte)) {
            Some(pos) => {
                if let Some(rest) = ws.get(pos + 1..) {
                    self.write(rest);
                }
            }
            None => self.write(ws),
        }
    }

    #[inline(always)]
    pub(crate) fn fits(&self, len: usize) -> bool {
        !self.can_fold() || self.column + len <= FOLD_TARGET
    }

    #[inline(always)]
    pub(crate) fn certainly_fits(&self, len: usize) -> bool {
        self.column + self.separator_len() + len <= FOLD_TARGET
    }

    #[inline(always)]
    fn take_word_budget(
        &mut self,
        overhead: usize,
        min_payload: usize,
        reserve: usize,
        bounds: (usize, usize),
        whole: impl FnOnce() -> usize,
    ) -> usize {
        let separator = self.separator_len();
        let room = FOLD_TARGET.saturating_sub(self.column + separator + reserve);
        let (lower, upper) = bounds;

        let keep_line = !self.can_fold()
            || room >= MAX_ENCODED_WORD
            || room >= upper
            || (room >= lower.min(overhead + min_payload) && {
                let whole = whole();
                room >= if whole <= MAX_ENCODED_WORD {
                    whole
                } else {
                    overhead + min_payload
                }
            });

        let room = if keep_line {
            self.begin_atom(0);
            room
        } else {
            match self.pending {
                Pending::Semicolon => self.output.write(b";\r\n "),
                _ => self.output.write(b"\r\n "),
            }
            self.pending = Pending::None;
            self.column = 1;
            self.line_start = 1;
            FOLD_TARGET - 1 - reserve
        };

        room.min(MAX_ENCODED_WORD)
            .saturating_sub(overhead)
            .max(min_payload)
    }

    #[inline(always)]
    pub(crate) fn finish(self) {
        self.output.write(b"\r\n");
    }
}

#[inline(always)]
pub(crate) const fn zero_lane(word: u64) -> u64 {
    const ONES: u64 = u64::from_ne_bytes([1; 8]);
    const HIGH: u64 = u64::from_ne_bytes([0x80; 8]);
    word.wrapping_sub(ONES) & !word & HIGH
}

#[inline(always)]
fn has_line_break(bytes: &[u8]) -> bool {
    const CR: u64 = u64::from_ne_bytes([b'\r'; 8]);
    const LF: u64 = u64::from_ne_bytes([b'\n'; 8]);

    let (chunks, tail) = bytes.as_chunks::<8>();
    let mut found = 0;

    for chunk in chunks {
        let word = u64::from_ne_bytes(*chunk);
        found |= zero_lane(word ^ CR) | zero_lane(word ^ LF);
    }

    if !tail.is_empty() {
        match bytes.last_chunk::<8>() {
            Some(last) => {
                let word = u64::from_ne_bytes(*last);
                found |= zero_lane(word ^ CR) | zero_lane(word ^ LF);
            }
            None => {
                return found != 0 || tail.iter().any(|&byte| is_line_break(byte));
            }
        }
    }

    found != 0
}

/// Writes `value` as unstructured text, folding before whitespace runs and
/// keeping every run so that unfolding restores the original value.
pub(crate) fn write_unstructured<W: Writer>(folder: &mut FoldWriter<'_, W>, value: &[u8]) {
    if folder.column + value.len() <= FOLD_TARGET && !has_line_break(value) {
        folder.write(value);
        return;
    }

    let mut rest = value;

    while !rest.is_empty() {
        let ws_len = rest
            .iter()
            .position(|&byte| !is_break(byte))
            .unwrap_or(rest.len());
        let (ws, after) = rest.split_at(ws_len);
        let atom_len = after
            .iter()
            .position(|&byte| is_break(byte))
            .unwrap_or(after.len());

        if after.is_empty() {
            folder.trailing_ws(ws);
            return;
        }

        folder.keep_ws(ws, atom_len);
        let (atom, next) = after.split_at(atom_len);
        folder.write(atom);
        rest = next;
    }
}

const fn q_len_table(phrase: bool) -> [u8; 256] {
    let mut table = [0u8; 256];
    let mut byte = 0;

    while byte < 256 {
        table[byte] = if phrase {
            quoted_printable_phrase_byte_len(byte as u8)
        } else {
            quoted_printable_byte_len(byte as u8)
        } as u8;
        byte += 1;
    }

    table
}

const fn q_literal_table(phrase: bool) -> [u8; 256] {
    let lengths = q_len_table(phrase);
    let mut table = [0u8; 256];
    let mut byte = 0;

    while byte < 256 {
        table[byte] = (lengths[byte] == 1 && byte != b' ' as usize) as u8;
        byte += 1;
    }

    table
}

const Q_LEN: [u8; 256] = q_len_table(false);
const Q_PHRASE_LEN: [u8; 256] = q_len_table(true);
const Q_LITERAL: [u8; 256] = q_literal_table(false);
const Q_PHRASE_LITERAL: [u8; 256] = q_literal_table(true);

#[inline(always)]
fn q_len_bytes<const PHRASE: bool>() -> &'static [u8; 256] {
    if PHRASE { &Q_PHRASE_LEN } else { &Q_LEN }
}

#[inline(always)]
pub(crate) fn q_byte_len<const PHRASE: bool>(byte: u8) -> usize {
    q_len_bytes::<PHRASE>()[byte as usize] as usize
}

#[inline(always)]
fn q_encode_byte<const PHRASE: bool>(byte: u8, output: &mut impl Writer) -> usize {
    if PHRASE {
        quoted_printable_encode_phrase_byte(byte, output)
    } else {
        quoted_printable_encode_byte(byte, output)
    }
}

#[inline(always)]
fn q_encode_slice<const PHRASE: bool>(input: &[u8], output: &mut impl Writer) -> usize {
    if PHRASE {
        phrase_quoted_printable_encode(input, output)
    } else {
        inline_quoted_printable_encode(input, output)
    }
}

#[inline(always)]
fn q_whole_len<const PHRASE: bool>(text: &str, max: usize) -> usize {
    if text.len() > max {
        return usize::MAX;
    }

    let table = q_len_bytes::<PHRASE>();
    let len: usize = text
        .as_bytes()
        .iter()
        .map(|&byte| table[byte as usize] as usize)
        .sum();

    if len > max { usize::MAX } else { len }
}

#[inline(always)]
fn is_literal<const PHRASE: bool>(byte: u8) -> bool {
    let table = if PHRASE {
        &Q_PHRASE_LITERAL
    } else {
        &Q_LITERAL
    };
    table[byte as usize] != 0
}

#[inline(always)]
const fn char_len(byte: u8) -> usize {
    match byte.leading_ones() {
        0 | 1 => 1,
        ones => ones as usize,
    }
}

/// Writes as much of `text` as fits in `budget` encoded characters, copying
/// runs that the "Q" encoding leaves untouched and escaping whole UTF-8
/// characters. Returns the number of input bytes consumed.
fn write_q_payload<W: Writer, const PHRASE: bool>(
    output: &mut W,
    text: &str,
    budget: usize,
    consumed: &mut usize,
) -> usize {
    let mut rest = text.as_bytes();
    let mut len = 0;

    loop {
        let run = rest
            .iter()
            .position(|&byte| !is_literal::<PHRASE>(byte))
            .unwrap_or(rest.len());
        let take = run.min(budget - len);

        if take > 0 {
            let (head, tail) = rest.split_at(take);
            if take < SHORT_RUN_LEN {
                for &byte in head {
                    output.write_byte(byte);
                }
            } else {
                output.write(head);
            }
            len += take;
            rest = tail;
        }

        if take < run {
            break;
        }

        let Some(&byte) = rest.first() else {
            break;
        };
        let chars = char_len(byte);
        let cost = if chars == 1 {
            q_byte_len::<PHRASE>(byte)
        } else {
            chars * 3
        };
        if len + cost > budget {
            break;
        }

        let (head, tail) = rest.split_at(chars.min(rest.len()));
        if chars == 1 {
            q_encode_byte::<PHRASE>(byte, output);
        } else {
            q_encode_slice::<PHRASE>(head, output);
        }
        len += cost;
        rest = tail;
    }

    *consumed = text.len() - rest.len();
    len
}

/// Writes `text` as a sequence of RFC 2047 "Q" encoded words, at most 75
/// characters each, split only at UTF-8 character boundaries.
pub(crate) fn write_q_words<W: Writer, const PHRASE: bool>(
    folder: &mut FoldWriter<'_, W>,
    text: &str,
    is_ascii: bool,
    tail: &[u8],
) {
    let overhead = if is_ascii {
        Q_ASCII.len()
    } else {
        Q_UTF8.len()
    } + 2;
    let max_payload = MAX_ENCODED_WORD - overhead;
    let mut rest = text;

    loop {
        let bounds = (
            overhead.saturating_add(rest.len()),
            overhead.saturating_add(rest.len().saturating_mul(3)),
        );
        let budget = folder.take_word_budget(overhead, MAX_Q_CHAR_LEN, tail.len(), bounds, || {
            overhead.saturating_add(q_whole_len::<PHRASE>(rest, max_payload))
        });
        if is_ascii {
            folder.write(Q_ASCII);
        } else {
            folder.write(Q_UTF8);
        }
        let mut consumed = 0;
        folder.write_encoded(|output| {
            write_q_payload::<W, PHRASE>(output, rest, budget, &mut consumed)
        });
        folder.write(b"?=");

        rest = rest.get(consumed..).unwrap_or_default();
        if rest.is_empty() {
            folder.write_tail(tail);
            return;
        }

        folder.space();
    }
}

/// Writes `text` as a sequence of RFC 2047 "B" encoded words, at most 75
/// characters each, split only at UTF-8 character boundaries.
#[inline(never)]
pub(crate) fn write_b_words<W: Writer>(folder: &mut FoldWriter<'_, W>, text: &str, tail: &[u8]) {
    let overhead = B_OVERHEAD;
    let whole = overhead + base64_encoded_len(text.len());

    if whole <= MAX_ENCODED_WORD {
        folder.begin_atom(whole + tail.len());
        folder.write(B_UTF8);
        folder.write_encoded(|output| base64_encode_inline(text.as_bytes(), output));
        folder.write(b"?=");
        folder.write_tail(tail);
        return;
    }

    let mut rest = text;

    loop {
        let whole = overhead.saturating_add(base64_encoded_len(rest.len()));
        let budget =
            folder.take_word_budget(overhead, MAX_B_CHAR_LEN, tail.len(), (whole, whole), || {
                whole
            });
        let mut consumed = (budget / 4 * 3).min(rest.len());
        while consumed > 0 && !rest.is_char_boundary(consumed) {
            consumed -= 1;
        }
        let (chunk, next) = rest.split_at_checked(consumed).unwrap_or((rest, ""));

        folder.write(B_UTF8);
        folder.write_encoded(|output| base64_encode_inline(chunk.as_bytes(), output));
        folder.write(b"?=");

        if next.is_empty() {
            folder.write_tail(tail);
            return;
        }

        folder.space();
        rest = next;
    }
}
