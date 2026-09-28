/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use crate::writer::Writer;
use encodify::{
    qp,
    rfc2047::{self, B},
};

/// Line length RFC 5322 section 2.1.1 recommends staying below.
pub(crate) const FOLD_TARGET: usize = 78;

pub(crate) const MAX_ENCODED_WORD: usize = 75;
const ENCODED_LINE_TARGET: usize = 76;
const Q_UTF8: &[u8] = b"=?utf-8?Q?";
const Q_ASCII: &[u8] = b"=?us-ascii?Q?";
const B_UTF8: &[u8] = b"=?utf-8?B?";
const WORD_END: &[u8] = b"?=";
const B_OVERHEAD: usize = B_UTF8.len() + WORD_END.len();
const MAX_Q_CHAR_LEN: usize = 12;
const MAX_B_CHAR_LEN: usize = 8;
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
const PENDING_NONE: u8 = 0;
const PENDING_SPACE: u8 = 1;
const PENDING_SEMICOLON: u8 = 2;
const SEPARATOR_MASK: u8 = 3;
const ENCODED_LINE: u8 = 4;

#[inline(always)]
const fn line_limit(pending: u8) -> usize {
    FOLD_TARGET - ((pending & ENCODED_LINE) >> 1) as usize
}

pub(crate) struct FoldWriter<'x, W: Writer> {
    output: &'x mut W,
    column: usize,
    line_start: usize,
    pending: u8,
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
                PENDING_SPACE
            } else {
                PENDING_NONE
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
    pub(crate) fn append_with<T>(
        &mut self,
        len: usize,
        append: impl FnOnce(&mut Vec<u8>) -> T,
    ) -> T {
        let mut appended = 0;
        let result = self.output.append_with(len, |buffer| {
            let start = buffer.len();
            let result = append(buffer);
            appended = buffer.len() - start;
            result
        });
        self.column += appended;
        result
    }

    #[inline(always)]
    fn write_word(
        &mut self,
        prefix: &[u8],
        budget: usize,
        payload: impl FnOnce(&mut Vec<u8>) -> usize,
    ) -> usize {
        self.append_with(prefix.len() + budget + WORD_END.len(), |buffer| {
            buffer.extend_from_slice(prefix);
            let consumed = payload(buffer);
            buffer.extend_from_slice(WORD_END);
            consumed
        })
    }

    #[inline(always)]
    pub(crate) fn space(&mut self) {
        self.pending = (self.pending & ENCODED_LINE) | PENDING_SPACE;
    }

    #[inline(always)]
    pub(crate) fn semicolon(&mut self) {
        self.pending = (self.pending & ENCODED_LINE) | PENDING_SEMICOLON;
    }

    #[inline(always)]
    fn separator_len(&self) -> usize {
        (self.pending & SEPARATOR_MASK) as usize
    }

    #[inline(always)]
    pub(crate) fn begin_atom(&mut self, len: usize) {
        let pending = self.pending;
        let separator = (pending & SEPARATOR_MASK) as usize;
        if separator == 0 {
            return;
        }

        if self.can_fold() && self.column + separator + len > line_limit(pending) {
            if separator == 2 {
                self.output.write(b";\r\n ");
            } else {
                self.output.write(b"\r\n ");
            }
            self.pending = PENDING_NONE;
            self.column = 1;
            self.line_start = 1;
        } else {
            if separator == 2 {
                self.output.write(b"; ");
            } else {
                self.output.write_byte(b' ');
            }
            self.pending = pending & ENCODED_LINE;
            self.column += separator;
        }
    }

    #[inline(always)]
    fn fold_or_write(&mut self, ws: &[u8], atom_len: usize) {
        if self.can_fold() && self.column + ws.len() + atom_len > line_limit(self.pending) {
            match ws {
                [b' '] => self.output.write(b"\r\n "),
                _ => {
                    self.output.write(b"\r\n");
                    self.output.write(ws);
                }
            }
            self.pending = PENDING_NONE;
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
                    self.pending = PENDING_NONE;
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
        !self.can_fold() || self.column + len <= line_limit(self.pending)
    }

    #[inline(always)]
    pub(crate) fn certainly_fits(&self, len: usize) -> bool {
        self.column + self.separator_len() + len <= line_limit(self.pending)
    }

    #[inline(always)]
    pub(crate) fn word_fits(&self, len: usize) -> bool {
        self.column + self.separator_len() + len <= ENCODED_LINE_TARGET
            || (self.can_fold() && len < ENCODED_LINE_TARGET)
    }

    #[inline(always)]
    pub(crate) fn begin_word(&mut self, len: usize) {
        self.pending |= ENCODED_LINE;
        self.begin_atom(len);
        self.pending |= ENCODED_LINE;
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
        let room = ENCODED_LINE_TARGET.saturating_sub(self.column + separator + reserve);
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
            if self.pending & SEPARATOR_MASK == PENDING_SEMICOLON {
                self.output.write(b";\r\n ");
            } else {
                self.output.write(b"\r\n ");
            }
            self.column = 1;
            self.line_start = 1;
            ENCODED_LINE_TARGET - 1 - reserve
        };
        self.pending = ENCODED_LINE;

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
    if folder.column + value.len() <= line_limit(folder.pending) && !has_line_break(value) {
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

impl<W: Writer> FoldWriter<'_, W> {
    /// Writes `text` as a sequence of RFC 2047 "Q" encoded words, at most 75
    /// characters each, split only at UTF-8 character boundaries.
    pub(crate) fn write_q_words<const PHRASE: bool>(
        &mut self,
        text: &str,
        is_ascii: bool,
        tail: &[u8],
    ) {
        let (words, q) = if PHRASE {
            (rfc2047::Q_PHRASE, qp::Q_PHRASE)
        } else {
            (rfc2047::Q_TEXT, qp::Q_TEXT)
        };
        let prefix = if is_ascii { Q_ASCII } else { Q_UTF8 };
        let overhead = prefix.len() + WORD_END.len();
        let max_payload = MAX_ENCODED_WORD - overhead;
        let mut rest = text;

        loop {
            let bounds = (
                overhead.saturating_add(rest.len()),
                overhead.saturating_add(rest.len().saturating_mul(3)),
            );
            let budget =
                self.take_word_budget(overhead, MAX_Q_CHAR_LEN, tail.len(), bounds, || {
                    q.encoded_len_within(rest, max_payload)
                        .map_or(usize::MAX, |len| overhead + len)
                });
            let consumed = self.write_word(prefix, budget, |buffer| {
                words.encode_payload(rest, budget, buffer)
            });

            rest = rest.get(consumed..).unwrap_or_default();
            if rest.is_empty() {
                self.write_tail(tail);
                return;
            }

            self.space();
        }
    }

    /// Writes `text` as a sequence of RFC 2047 "B" encoded words, at most 75
    /// characters each, split only at UTF-8 character boundaries.
    #[inline(never)]
    pub(crate) fn write_b_words(&mut self, text: &str, tail: &[u8]) {
        let payload = B.payload_len(text);
        let whole = B_OVERHEAD + payload;

        if whole <= MAX_ENCODED_WORD && self.word_fits(whole + tail.len()) {
            self.begin_word(whole + tail.len());
            self.write_word(B_UTF8, payload, |buffer| {
                B.encode_payload(text, payload, buffer)
            });
            self.write_tail(tail);
            return;
        }

        let mut rest = text;

        loop {
            let whole = B_OVERHEAD.saturating_add(B.payload_len(rest));
            let budget = self.take_word_budget(
                B_OVERHEAD,
                MAX_B_CHAR_LEN,
                tail.len(),
                (whole, whole),
                || whole,
            );
            let consumed = self.write_word(B_UTF8, budget, |buffer| {
                B.encode_payload(rest, budget, buffer)
            });

            rest = rest.get(consumed..).unwrap_or_default();
            if rest.is_empty() {
                self.write_tail(tail);
                return;
            }

            self.space();
        }
    }
}
