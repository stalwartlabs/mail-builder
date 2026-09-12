/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use crate::{
    encoders::encode::{EncodingType, get_encoding_type},
    headers::{
        Header,
        address::{Address, EmailAddress, GroupedAddresses},
        content_type::ContentType,
        date::Date,
        message_id::MessageId,
        raw::Raw,
        text::Text,
        url::URL,
    },
};
use mail_parser::{MessageParser, MimeHeaders};

const CASES: usize = 5_000;
const DATE_CASES: usize = 50_000;
const MAX_LINE: usize = 78;
const MAX_ENCODED_WORD: usize = 75;

pub(crate) struct Prng(u64);

impl Prng {
    fn new(seed: u64) -> Self {
        Prng(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut state = self.0;
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        self.0 = state;
        state
    }

    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound.max(1) as u64) as usize
    }

    fn range(&mut self, low: usize, high: usize) -> usize {
        low + self.below(high - low + 1)
    }

    fn chance(&mut self, one_in: usize) -> bool {
        self.below(one_in) == 0
    }

    fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }
}

const ASCII_WORDS: &[&str] = &[
    "report",
    "Quarterly",
    "John",
    "Doe",
    "meeting",
    "x",
    "ab",
    "consolidated",
    "figures",
    "forecast",
    "headcount",
    "action-items",
    "2026",
    "Q3",
    "re:",
    "fwd",
    "a",
    "the",
];
const LATIN_CHARS: &[char] = &[
    'é', 'è', 'ü', 'ß', 'ñ', 'å', 'ø', 'î', 'ç', 'Δ', 'Ω', 'λ', 'ж', 'щ', 'א', 'ت',
];
const CJK_CHARS: &[char] = &[
    '안', '녕', '하', '세', '요', '世', '界', '添', '付', '日', '本', '語', '中', '文', 'こ', 'ん',
    'に', 'ち', 'は', '😀', '🌍',
];
const SPECIAL_CHARS: &[char] = &[
    '(', ')', '<', '>', '[', ']', ':', ';', '@', '\\', ',', '"', '.', '?', '=', '_', '\'', '!',
];
const CONTROL_CHARS: &[char] = &[
    '\0', '\u{1}', '\u{3}', '\u{7}', '\u{8}', '\t', '\n', '\u{b}', '\u{c}', '\r', '\u{e}',
    '\u{1f}', '\u{7f}',
];
const ATEXT: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789-_.";

fn push_word(prng: &mut Prng, out: &mut String, pool: usize, len: usize) {
    match pool {
        0 => out.push_str(prng.pick(ASCII_WORDS)),
        1 => {
            for _ in 0..len {
                out.push(prng.pick(LATIN_CHARS));
            }
        }
        2 => {
            for _ in 0..len {
                out.push(prng.pick(CJK_CHARS));
            }
        }
        3 => {
            for _ in 0..len {
                out.push(prng.pick(SPECIAL_CHARS));
            }
        }
        4 => {
            for _ in 0..len {
                match prng.below(4) {
                    0 => out.push_str(prng.pick(ASCII_WORDS)),
                    1 => out.push(prng.pick(LATIN_CHARS)),
                    2 => out.push(prng.pick(CJK_CHARS)),
                    _ => out.push(prng.pick(SPECIAL_CHARS)),
                }
            }
        }
        5 => {
            for _ in 0..len {
                match prng.below(6) {
                    0 | 1 => out.push(prng.pick(CONTROL_CHARS)),
                    2 => out.push('"'),
                    3 => out.push('\\'),
                    4 => out.push(prng.pick(LATIN_CHARS)),
                    _ => out.push(prng.pick(SPECIAL_CHARS)),
                }
            }
        }
        6 => {
            for _ in 0..len {
                out.push(prng.pick(CONTROL_CHARS));
            }
        }
        _ => {
            for _ in 0..len {
                out.push(char::from(b'a' + prng.below(26) as u8));
            }
        }
    }
}

/// Display names and subjects: every alphabet, whitespace runs, long words
/// and leading or trailing spaces.
fn gen_phrase(prng: &mut Prng) -> String {
    let mut out = String::new();
    let words = prng.range(0, 12);
    let pool = prng.below(8);

    if prng.chance(6) {
        for _ in 0..prng.range(1, 3) {
            out.push(if prng.chance(3) { '\t' } else { ' ' });
        }
    }

    for word in 0..words {
        if word > 0 {
            let run = if prng.chance(4) { 6 } else { 1 };
            for _ in 0..prng.range(1, run) {
                out.push(if prng.chance(4) { '\t' } else { ' ' });
            }
        }
        let len = if prng.chance(12) {
            prng.range(40, 220)
        } else {
            prng.range(1, 10)
        };
        push_word(prng, &mut out, pool, len);
    }

    if prng.chance(6) {
        for _ in 0..prng.range(1, 3) {
            out.push(if prng.chance(3) { '\t' } else { ' ' });
        }
    }

    if prng.chance(8) {
        out.push_str(match prng.below(3) {
            0 => "\r\n",
            1 => "\r",
            _ => "\n",
        });
    }

    out
}

fn gen_atom(prng: &mut Prng, low: usize, high: usize) -> String {
    let len = prng.range(low, high);
    (0..len)
        .map(|_| char::from(prng.pick(ATEXT)))
        .collect::<String>()
}

fn gen_email(prng: &mut Prng) -> String {
    let local = if prng.chance(20) { 60 } else { 12 };
    let domain = if prng.chance(20) { 60 } else { 12 };
    let local = gen_atom(prng, 1, local);
    let domain = gen_atom(prng, 1, domain);
    let tld = ["com", "org", "example", "co.uk"][prng.below(4)];
    format!("{local}@{domain}.{tld}")
}

fn gen_id(prng: &mut Prng) -> String {
    let head = prng.next();
    let mid = gen_atom(prng, 1, 20);
    let host = gen_email(prng);
    let host = host.split('@').nth(1).unwrap_or("example.org");
    format!("{head:016x}.{mid}@{host}")
}

fn gen_url(prng: &mut Prng) -> String {
    match prng.below(3) {
        0 => {
            let path = gen_atom(prng, 1, 30);
            let token = gen_atom(prng, 4, 40);
            format!("https://example.org/{path}?token={token}")
        }
        1 => format!("mailto:{}", gen_email(prng)),
        _ => {
            let host = gen_email(prng);
            let host = host.split('@').nth(1).unwrap_or("x.org").to_string();
            let path = gen_atom(prng, 1, 60);
            format!("http://{host}/{path}")
        }
    }
}

fn gen_filename(prng: &mut Prng) -> String {
    let mut name = gen_phrase(prng);
    if prng.chance(2) {
        name.push_str(".pdf");
    }
    name
}

fn gen_raw(prng: &mut Prng) -> String {
    let mut out = gen_phrase(prng);
    if prng.chance(5) {
        out.push_str("\r\n\t");
        out.push_str(&gen_phrase(prng));
    }
    if prng.chance(8) {
        out.push('\r');
        out.push_str(&gen_phrase(prng));
    }
    if prng.chance(8) {
        out.push('\n');
    }
    out
}

fn build(name: &str, header: &impl Header) -> Vec<u8> {
    let mut out = Vec::with_capacity(256);
    out.extend_from_slice(name.as_bytes());
    out.extend_from_slice(b": ");
    header.write_header(&mut out, name.len() + 2);
    out
}

fn check(name: &str, header: &impl Header, allow_trailing_ws: bool) -> Vec<u8> {
    let out = build(name, header);
    validate(&out, name.len() + 2, allow_trailing_ws);
    out
}

fn is_wsp(byte: u8) -> bool {
    byte == b' ' || byte == b'\t'
}

fn lines(header: &[u8]) -> Vec<&[u8]> {
    let mut lines = Vec::new();
    let mut rest = header;
    while let Some(pos) = rest.windows(2).position(|pair| pair == b"\r\n") {
        lines.push(&rest[..pos]);
        rest = &rest[pos + 2..];
    }
    if !rest.is_empty() {
        lines.push(rest);
    }
    lines
}

fn decode_base64(input: &[u8]) -> Option<Vec<u8>> {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = Vec::new();
    let mut acc = 0u32;
    let mut bits = 0;
    for &byte in input.iter().filter(|&&byte| byte != b'=') {
        let value = ALPHABET.iter().position(|&ch| ch == byte)? as u32;
        acc = (acc << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits) as u8);
        }
    }
    Some(out)
}

fn decode_q(input: &[u8]) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut rest = input;
    while let Some((&byte, tail)) = rest.split_first() {
        match byte {
            b'_' => {
                out.push(b' ');
                rest = tail;
            }
            b'=' => {
                let (hex, tail) = tail.split_at_checked(2)?;
                out.push(u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?);
                rest = tail;
            }
            _ => {
                out.push(byte);
                rest = tail;
            }
        }
    }
    Some(out)
}

struct Word {
    #[allow(dead_code)]
    text: String,
    len: usize,
    quoted: bool,
}

fn encoded_words(line: &[u8]) -> Vec<Word> {
    let mut words = Vec::new();
    let mut rest = line;
    let mut offset = 0;
    while let Some(start) = rest.windows(2).position(|pair| pair == b"=?") {
        let body = &rest[start..];
        let head = match body
            .iter()
            .enumerate()
            .filter(|&(_, &byte)| byte == b'?')
            .nth(2)
        {
            Some((pos, _)) => pos + 1,
            None => break,
        };
        let end = match body.windows(2).skip(head).position(|pair| pair == b"?=") {
            Some(end) => head + end + 2,
            None => break,
        };
        let word = &body[..end];
        let charset = &word[2..word[2..].iter().position(|&byte| byte == b'?').unwrap_or(0) + 2];
        if !matches!(charset, b"utf-8" | b"us-ascii") {
            offset += start + 2;
            rest = &body[2..];
            continue;
        }
        let quoted = offset + start > 0
            && line.get(offset + start - 1) == Some(&b'"')
            && line.get(offset + start + end) == Some(&b'"');
        let parts: Vec<&[u8]> = word[2..word.len() - 2]
            .splitn(3, |&byte| byte == b'?')
            .collect();
        assert_eq!(
            parts.len(),
            3,
            "malformed encoded word {:?}",
            String::from_utf8_lossy(word)
        );
        let decoded = match parts[1] {
            b"Q" | b"q" => decode_q(parts[2]),
            b"B" | b"b" => decode_base64(parts[2]),
            _ => None,
        }
        .unwrap_or_else(|| {
            panic!(
                "encoded word does not decode: {:?}",
                String::from_utf8_lossy(word)
            )
        });
        let text = String::from_utf8(decoded).unwrap_or_else(|err| {
            panic!(
                "encoded word {:?} does not decode to UTF-8: {err}",
                String::from_utf8_lossy(word)
            )
        });
        words.push(Word {
            text,
            len: word.len(),
            quoted,
        });
        offset += start + end;
        rest = &body[end..];
    }
    words
}

/// Checks every rule the folder is required to keep: CRLF line ends, no bare
/// CR or LF, continuation lines starting with whitespace, no empty
/// continuation, no trailing whitespace before a fold, the 78 character
/// target unless a single atom is longer, and RFC 2047 encoded words of at
/// most 75 characters that each decode standalone.
fn validate(header: &[u8], column: usize, allow_trailing_ws: bool) {
    let text = String::from_utf8_lossy(header).into_owned();
    assert!(
        header.ends_with(b"\r\n"),
        "header does not end with CRLF: {text:?}"
    );
    let body = &header[..header.len() - 2];

    for (pos, line) in lines(body).iter().enumerate() {
        assert!(
            !line.contains(&b'\r') && !line.contains(&b'\n'),
            "bare CR or LF in {text:?}"
        );
        assert!(!line.is_empty(), "empty line in {text:?}");

        let leading = line.iter().take_while(|&&byte| is_wsp(byte)).count();
        if pos > 0 {
            assert!(leading > 0, "continuation line without WSP in {text:?}");
        }
        assert!(leading < line.len(), "whitespace only line in {text:?}");

        let is_last = pos + 1 == lines(body).len();
        let trailing = line.iter().rev().take_while(|&&byte| is_wsp(byte)).count();
        let is_empty_value = pos == 0 && line.len() <= column;
        if (!is_last || !allow_trailing_ws) && !is_empty_value {
            assert_eq!(trailing, 0, "trailing whitespace before CRLF in {text:?}");
        }

        let value = if pos == 0 { column } else { 0 };
        let start = value
            + line
                .get(value..)
                .unwrap_or_default()
                .iter()
                .take_while(|&&byte| is_wsp(byte))
                .count();
        let content = line.get(start..line.len() - trailing).unwrap_or_default();
        if line.len() - trailing > MAX_LINE {
            assert!(
                !content.iter().any(|&byte| is_wsp(byte)),
                "line of {} bytes could have been folded in {text:?}",
                line.len()
            );
        }

        for word in encoded_words(line) {
            assert!(
                word.quoted || word.len <= MAX_ENCODED_WORD,
                "encoded word of {} characters in {text:?}",
                word.len
            );
        }
    }
}

fn collapse(value: &str) -> String {
    value.split_ascii_whitespace().collect::<Vec<_>>().join(" ")
}

fn is_encoded(value: &str) -> bool {
    !matches!(
        get_encoding_type(value.as_bytes(), true, false),
        EncodingType::None
    )
}

fn parse_header<'x>(header: &'x [u8], name: &str) -> mail_parser::Message<'x> {
    MessageParser::new()
        .parse_headers(header)
        .unwrap_or_else(|| {
            panic!(
                "{name} header did not parse: {:?}",
                String::from_utf8_lossy(header)
            )
        })
}

#[test]
fn quoted_printable_length_tables_match_the_encoders() {
    for byte in 0..=u8::MAX {
        let mut inline = Vec::new();
        let mut phrase = Vec::new();
        let inline_len =
            crate::encoders::quoted_printable::quoted_printable_encode_byte(byte, &mut inline);
        let phrase_len = crate::encoders::quoted_printable::quoted_printable_encode_phrase_byte(
            byte,
            &mut phrase,
        );

        assert_eq!(inline_len, inline.len(), "byte {byte}");
        assert_eq!(phrase_len, phrase.len(), "byte {byte}");
        assert_eq!(
            crate::headers::fold::q_byte_len::<false>(byte),
            inline_len,
            "byte {byte}"
        );
        assert_eq!(
            crate::headers::fold::q_byte_len::<true>(byte),
            phrase_len,
            "byte {byte}"
        );
    }
}

#[test]
fn text_round_trips() {
    let mut prng = Prng::new(0x5eed_1234_abcd_0001);

    for case in 0..CASES {
        let value = gen_phrase(&mut prng);
        let header = check("Subject", &Text::new(value.as_str()), false);

        let message = parse_header(&header, "Subject");
        let parsed = message.subject().unwrap_or_default();

        if is_encoded(&value) {
            assert_eq!(
                parsed,
                value,
                "case {case} of {value:?} -> {:?}",
                String::from_utf8_lossy(&header)
            );
        } else if !header.windows(2).any(|pair| pair == b"=?") {
            let breaks = value.bytes().any(|byte| byte == b'\r' || byte == b'\n');
            let folded = lines(&header).len() > 1 || breaks;
            let expected = if folded {
                collapse(&value)
            } else {
                value.trim_matches(|ch| ch == ' ' || ch == '\t').to_string()
            };
            let parsed = if folded {
                collapse(parsed)
            } else {
                parsed.to_string()
            };
            assert_eq!(
                parsed,
                expected,
                "case {case} of {value:?} -> {:?}",
                String::from_utf8_lossy(&header)
            );
        }
    }
}

#[test]
fn raw_stays_valid() {
    let mut prng = Prng::new(0x5eed_1234_abcd_0002);

    for _ in 0..CASES {
        let value = gen_raw(&mut prng);
        let header = check("X-Raw", &Raw::new(value.as_str()), true);

        let message = parse_header(&header, "X-Raw");
        let parsed = message
            .header("X-Raw")
            .and_then(|value| value.as_text())
            .unwrap_or_default();
        assert_eq!(
            collapse(parsed),
            collapse(&value),
            "{value:?} -> {:?}",
            String::from_utf8_lossy(&header)
        );
    }
}

fn strip_line_breaks(value: &str) -> String {
    value.replace(['\r', '\n'], "")
}

fn address_expectation(name: &str, encoded: bool) -> String {
    let name = strip_line_breaks(name);
    if encoded { name } else { collapse(&name) }
}

#[test]
fn addresses_round_trip() {
    let mut prng = Prng::new(0x5eed_1234_abcd_0003);

    for _ in 0..CASES {
        let count = prng.range(1, 6);
        let mut expected = Vec::new();
        let mut list = Vec::new();

        for _ in 0..count {
            let email = gen_email(&mut prng);
            let name = if prng.chance(4) {
                None
            } else {
                Some(gen_phrase(&mut prng))
            };
            expected.push((name.clone(), email.clone()));
            list.push(Address::new_address(name, email));
        }

        let header = check("To", &Address::new_list(list), false);

        let message = parse_header(&header, "To");
        let parsed: Vec<(Option<String>, Option<String>)> = message
            .to()
            .map(|to| {
                to.iter()
                    .map(|addr| {
                        (
                            addr.name().map(str::to_string),
                            addr.address().map(str::to_string),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        assert_eq!(
            parsed.len(),
            expected.len(),
            "{expected:?} -> {:?}",
            String::from_utf8_lossy(&header)
        );

        for (parsed, (name, email)) in parsed.iter().zip(expected.iter()) {
            assert_eq!(
                parsed.1.as_deref(),
                Some(email.as_str()),
                "{expected:?} -> {:?}",
                String::from_utf8_lossy(&header)
            );
            match name {
                Some(name) if !name.trim().is_empty() => {
                    let encoded = is_encoded(name);
                    assert_eq!(
                        parsed
                            .0
                            .as_deref()
                            .map(|parsed| address_expectation(parsed, encoded)),
                        Some(address_expectation(name, encoded)),
                        "{expected:?} -> {:?}",
                        String::from_utf8_lossy(&header)
                    )
                }
                _ => (),
            }
        }
    }
}

#[test]
fn groups_round_trip() {
    let mut prng = Prng::new(0x5eed_1234_abcd_0004);

    for _ in 0..CASES / 2 {
        let name = gen_phrase(&mut prng);
        let count = prng.range(0, 4);
        let mut expected = Vec::new();
        let mut members = Vec::new();

        for _ in 0..count {
            let email = gen_email(&mut prng);
            expected.push(email.clone());
            members.push(Address::new_address(None::<String>, email));
        }

        let header = check(
            "Cc",
            &Address::new_group(Some(name.clone()), members),
            false,
        );

        let message = parse_header(&header, "Cc");
        let groups = message
            .cc()
            .and_then(|cc| cc.as_group().map(|groups| groups.to_vec()))
            .unwrap_or_default();

        if name.trim().is_empty() && count == 0 {
            continue;
        }

        assert_eq!(
            groups.len(),
            1,
            "{name:?} -> {:?}",
            String::from_utf8_lossy(&header)
        );
        if !name.trim().is_empty() {
            let encoded = is_encoded(&name);
            assert_eq!(
                groups[0]
                    .name
                    .as_deref()
                    .map(|parsed| address_expectation(parsed, encoded)),
                Some(address_expectation(&name, encoded)),
                "{name:?} -> {:?}",
                String::from_utf8_lossy(&header)
            );
        }
        let addresses: Vec<&str> = groups[0]
            .addresses
            .iter()
            .filter_map(|addr| addr.address())
            .collect();
        assert_eq!(
            addresses,
            expected.iter().map(String::as_str).collect::<Vec<_>>(),
            "{name:?} -> {:?}",
            String::from_utf8_lossy(&header)
        );
    }
}

#[test]
fn nested_lists_do_not_panic() {
    let mut prng = Prng::new(0x5eed_1234_abcd_0005);

    for _ in 0..CASES / 10 {
        let inner = Address::new_list(
            (0..prng.range(0, 3))
                .map(|_| Address::new_address(Some(gen_phrase(&mut prng)), gen_email(&mut prng)))
                .collect(),
        );
        let group = Address::new_group(
            Some(gen_phrase(&mut prng)),
            vec![
                Address::new_list(vec![Address::new_address(
                    None::<String>,
                    gen_email(&mut prng),
                )]),
                Address::new_group(
                    Some(gen_phrase(&mut prng)),
                    vec![Address::new_address(None::<String>, gen_email(&mut prng))],
                ),
            ],
        );
        check(
            "Bcc",
            &Address::new_list(vec![inner, group, Address::new_list(Vec::new())]),
            false,
        );
    }
}

#[test]
fn content_type_round_trips() {
    let mut prng = Prng::new(0x5eed_1234_abcd_0006);

    for _ in 0..CASES {
        let c_type = ["text/plain", "multipart/mixed", "attachment", "inline"][prng.below(4)];
        let mut header_value = ContentType::new(c_type);
        let mut expected = Vec::new();

        for _ in 0..prng.range(0, 3) {
            let key = ["filename", "charset", "boundary", "name"][prng.below(4)];
            if expected.iter().any(|(name, _)| *name == key) {
                continue;
            }
            let value = gen_filename(&mut prng);
            expected.push((key, value.clone()));
            header_value = header_value.attribute(key, value);
        }

        let header = check("Content-Type", &header_value, false);

        let message = parse_header(&header, "Content-Type");
        let parsed = message.content_type().expect("no content type");

        for (key, value) in &expected {
            if value.trim().is_empty() {
                continue;
            }
            let parsed = parsed.attribute(key).unwrap_or_else(|| {
                panic!("missing {key} in {:?}", String::from_utf8_lossy(&header))
            });
            assert_eq!(
                collapse(&strip_line_breaks(parsed)),
                collapse(&strip_line_breaks(value)),
                "{expected:?} -> {:?}",
                String::from_utf8_lossy(&header)
            );
        }
    }
}

#[test]
fn message_ids_round_trip() {
    let mut prng = Prng::new(0x5eed_1234_abcd_0007);

    for _ in 0..CASES {
        let ids: Vec<String> = (0..prng.range(1, 6)).map(|_| gen_id(&mut prng)).collect();
        let header = check("References", &MessageId::from(ids.clone()), false);

        let message = parse_header(&header, "References");
        let parsed: Vec<&str> = match message.references() {
            mail_parser::HeaderValue::Text(id) => vec![id.as_ref()],
            mail_parser::HeaderValue::TextList(ids) => ids.iter().map(|id| id.as_ref()).collect(),
            _ => Vec::new(),
        };
        assert_eq!(
            parsed,
            ids.iter().map(String::as_str).collect::<Vec<_>>(),
            "{:?}",
            String::from_utf8_lossy(&header)
        );
    }
}

#[test]
fn urls_round_trip() {
    let mut prng = Prng::new(0x5eed_1234_abcd_0008);

    for _ in 0..CASES {
        let urls: Vec<String> = (0..prng.range(1, 4)).map(|_| gen_url(&mut prng)).collect();
        let header = check("List-Unsubscribe", &URL::from(urls.clone()), false);

        let message = parse_header(&header, "List-Unsubscribe");
        let parsed: Vec<String> = message
            .header("List-Unsubscribe")
            .and_then(|value| value.as_address())
            .map(|list| {
                list.iter()
                    .filter_map(|addr| addr.address().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        assert_eq!(parsed, urls, "{:?}", String::from_utf8_lossy(&header));
    }
}

#[test]
fn single_address_and_grouped_headers_validate() {
    let mut prng = Prng::new(0x5eed_1234_abcd_0009);

    for _ in 0..CASES / 10 {
        let address = EmailAddress {
            name: Some(gen_phrase(&mut prng).into()),
            email: gen_email(&mut prng).into(),
        };
        check("From", &address, false);

        let group = GroupedAddresses {
            name: Some(gen_phrase(&mut prng).into()),
            addresses: (0..prng.range(1, 3))
                .map(|_| Address::new_address(None::<String>, gen_email(&mut prng)))
                .collect(),
        };
        check("Cc", &group, false);
    }
}

#[test]
fn headers_fold_from_any_starting_column() {
    let mut prng = Prng::new(0x5eed_1234_abcd_000a);

    for column in [0usize, 1, 2, 8, 70, 76, 77, 78, 79, 120, 500] {
        for _ in 0..2000 {
            let value = gen_phrase(&mut prng);
            let mut out = vec![b'x'; column];
            Text::new(value.as_str()).write_header(&mut out, column);
            if column > 0 {
                validate(&out, column, false);
            }

            let mut out = vec![b'x'; column];
            Address::new_address(Some(value.as_str()), gen_email(&mut prng))
                .write_header(&mut out, column);
            if column > 0 {
                validate(&out, column, false);
            }
        }
    }
}

#[test]
fn date_matches_a_civil_calendar_reference() {
    let mut prng = Prng::new(0x5eed_1234_abcd_000b);
    let low = -2_208_988_800i64;
    let high = 7_258_118_400i64;

    for case in 0..DATE_CASES {
        let date = match case {
            0 => 0,
            1 => low,
            2 => high,
            3 => -1,
            4 => 86399,
            5 => -86400,
            6 => i64::MAX / 1000,
            7 => i64::MIN / 1000,
            _ => low + (prng.next() % (high - low) as u64) as i64,
        };

        let produced = Date::new(date).to_rfc822();
        let mut written = Vec::new();
        Date::new(date).write_rfc822(&mut written);
        assert_eq!(
            produced.as_bytes(),
            written.as_slice(),
            "to_rfc822 and write_rfc822 disagree for {date}"
        );

        assert_eq!(produced, civil_reference(date), "for timestamp {date}");
    }
}

fn civil_reference(date: i64) -> String {
    let days = date.div_euclid(86400);
    let seconds = date.rem_euclid(86400);
    let mut year = 1970i64;
    let mut left = days;

    let leap = |year: i64| (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    let days_in_year = |year: i64| if leap(year) { 366 } else { 365 };

    while left < 0 {
        year -= 1;
        left += days_in_year(year);
    }
    while left >= days_in_year(year) {
        left -= days_in_year(year);
        year += 1;
    }

    let months = [
        31,
        if leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut month = 0;
    while left >= months[month] {
        left -= months[month];
        month += 1;
    }

    format!(
        "{}, {} {} {:04} {:02}:{:02}:{:02} +0000",
        crate::headers::date::DOW[(days + 4).rem_euclid(7) as usize],
        left + 1,
        crate::headers::date::MONTH[month],
        year,
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
    )
}

#[test]
fn date_header_matches_to_rfc822() {
    let mut prng = Prng::new(0x5eed_1234_abcd_000c);

    for _ in 0..CASES {
        let date = (prng.next() % 4_000_000_000) as i64 - 2_000_000_000;
        let header = check("Date", &Date::new(date), false);
        assert_eq!(
            String::from_utf8_lossy(&header),
            format!("Date: {}\r\n", Date::new(date).to_rfc822())
        );
    }
}

#[test]
fn control_characters_never_reach_the_output() {
    let long = "a".repeat(40);
    let names: Vec<String> = [
        ":\n:\\",
        "\u{1}\n(\\",
        "\u{8}\0\0\n\\",
        "a\rb",
        "\r\n",
        "\\\n",
        "\"\n",
        "x\ty\nz\\",
    ]
    .iter()
    .flat_map(|name| {
        [
            name.to_string(),
            format!("{long}{name}"),
            format!("{name}{long}"),
        ]
    })
    .collect();

    for name in &names {
        let name = name.as_str();
        for header in [
            check("To", &Address::new_address(Some(name), "a@b.com"), false),
            check(
                "To",
                &Address::new_group(
                    Some(name),
                    vec![Address::new_address(None::<&str>, "a@b.com")],
                ),
                false,
            ),
            check(
                "Content-Type",
                &ContentType::new("attachment").attribute("filename", name),
                false,
            ),
        ] {
            let text = String::from_utf8_lossy(&header);
            assert_eq!(
                text.matches('\r').count(),
                text.matches("\r\n").count(),
                "bare CR in {text:?}"
            );
        }
    }
}
