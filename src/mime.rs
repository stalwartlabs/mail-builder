/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use crate::{
    encoders::{base64::base64_encode_wrapped, encode::write_encoded_body},
    headers::{
        Header, HeaderType, content_type::ContentType, message_id::MessageId, raw::Raw, text::Text,
    },
    writer::Writer,
};
use std::{
    borrow::Cow,
    cell::Cell,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

/// MIME part of an e-mail.
#[derive(Clone, Debug)]
pub struct MimePart<'x> {
    pub headers: Vec<(Cow<'x, str>, HeaderType<'x>)>,
    pub contents: BodyPart<'x>,
}

#[derive(Clone, Debug)]
pub enum BodyPart<'x> {
    Text(Cow<'x, str>),
    Binary(Cow<'x, [u8]>),
    Multipart(Vec<MimePart<'x>>),
}

impl<'x> From<&'x str> for BodyPart<'x> {
    fn from(value: &'x str) -> Self {
        BodyPart::Text(value.into())
    }
}

impl<'x> From<&'x [u8]> for BodyPart<'x> {
    fn from(value: &'x [u8]) -> Self {
        BodyPart::Binary(value.into())
    }
}

impl From<String> for BodyPart<'_> {
    fn from(value: String) -> Self {
        BodyPart::Text(value.into())
    }
}

impl<'x> From<&'x String> for BodyPart<'x> {
    fn from(value: &'x String) -> Self {
        BodyPart::Text(value.as_str().into())
    }
}

impl<'x> From<Cow<'x, str>> for BodyPart<'x> {
    fn from(value: Cow<'x, str>) -> Self {
        BodyPart::Text(value)
    }
}

impl From<Vec<u8>> for BodyPart<'_> {
    fn from(value: Vec<u8>) -> Self {
        BodyPart::Binary(value.into())
    }
}

impl<'x> From<Vec<MimePart<'x>>> for BodyPart<'x> {
    fn from(value: Vec<MimePart<'x>>) -> Self {
        BodyPart::Multipart(value)
    }
}

impl<'x> From<&'x str> for ContentType<'x> {
    fn from(value: &'x str) -> Self {
        ContentType::new(value)
    }
}

impl From<String> for ContentType<'_> {
    fn from(value: String) -> Self {
        ContentType::new(value)
    }
}

impl<'x> From<&'x String> for ContentType<'x> {
    fn from(value: &'x String) -> Self {
        ContentType::new(value.as_str())
    }
}

const HEX_DIGITS: &[u8; 16] = b"0123456789abcdef";
const GOLDEN_RATIO: u64 = 0x9E37_79B9_7F4A_7C15;
const BOUNDARY_HEX_LEN: usize = 16;

thread_local!(static BOUNDARY_STATE: Cell<(u64, u64)> = const { Cell::new((0, 0)) });
static THREAD_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static PROCESS_ENTROPY: AtomicU64 = AtomicU64::new(0);

#[inline(always)]
fn splitmix64(seed: u64) -> u64 {
    let mut z = seed;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[inline(always)]
fn push_hex(buffer: &mut [u8; BOUNDARY_HEX_LEN], end: usize, mut value: u64) -> usize {
    let mut at = end;
    while let Some(next) = at.checked_sub(1) {
        at = next;
        if let Some(slot) = buffer.get_mut(at) {
            *slot = HEX_DIGITS[(value & 15) as usize];
        }
        value >>= 4;
        if value == 0 {
            break;
        }
    }
    at
}

#[inline]
fn unix_nanos() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos() as u64)
}

fn process_entropy(address: u64, nanos: u64) -> u64 {
    let known = PROCESS_ENTROPY.load(Ordering::Relaxed);
    if known != 0 {
        return known;
    }
    let candidate = splitmix64(address ^ nanos.rotate_left(17)) | 1;
    match PROCESS_ENTROPY.compare_exchange(0, candidate, Ordering::Relaxed, Ordering::Relaxed) {
        Ok(_) => candidate,
        Err(existing) => existing,
    }
}

#[inline]
fn boundary_fields() -> (u64, u64, u64) {
    let nanos = unix_nanos();
    BOUNDARY_STATE.with(|state| {
        let (mut thread_seed, counter) = state.get();
        if thread_seed == 0 {
            let address = std::ptr::from_ref(state) as usize as u64;
            let sequence = THREAD_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            thread_seed = splitmix64(sequence) ^ process_entropy(address, nanos);
            if thread_seed == 0 {
                thread_seed = GOLDEN_RATIO;
            }
        }
        let counter = counter.wrapping_add(1);
        state.set((thread_seed, counter));
        (
            nanos,
            splitmix64(thread_seed ^ counter.wrapping_mul(GOLDEN_RATIO)),
            thread_seed,
        )
    })
}

/// Writes a pseudo-unique MIME boundary without allocating.
///
/// The three hexadecimal fields are only guaranteed distinct as a triple,
/// so `separator` should be a non-empty string without hexadecimal digits.
pub fn write_boundary(output: &mut impl Writer, separator: &str) {
    let (nanos, unique, thread_seed) = boundary_fields();

    let mut buffer = [0u8; BOUNDARY_HEX_LEN];
    let at = push_hex(&mut buffer, BOUNDARY_HEX_LEN, nanos);
    output.write(buffer.get(at..).unwrap_or_default());
    output.write(separator.as_bytes());
    let at = push_hex(&mut buffer, BOUNDARY_HEX_LEN, unique);
    output.write(buffer.get(at..).unwrap_or_default());
    output.write(separator.as_bytes());
    let at = push_hex(&mut buffer, BOUNDARY_HEX_LEN, thread_seed);
    output.write(buffer.get(at..).unwrap_or_default());
}

pub fn make_boundary(separator: &str) -> String {
    let mut boundary = Vec::with_capacity(BOUNDARY_HEX_LEN * 3 + separator.len() * 2);
    write_boundary(&mut boundary, separator);
    String::from_utf8(boundary).unwrap_or_default()
}

impl<'x> MimePart<'x> {
    /// Create a new MIME part.
    pub fn new(
        content_type: impl Into<ContentType<'x>>,
        contents: impl Into<BodyPart<'x>>,
    ) -> Self {
        let mut content_type = content_type.into();
        let contents = contents.into();

        if matches!(contents, BodyPart::Text(_)) && content_type.attributes.is_empty() {
            content_type.attributes.reserve_exact(1);
            content_type
                .attributes
                .push((Cow::from("charset"), Cow::from("utf-8")));
        }

        let mut headers = Vec::with_capacity(2);
        headers.push((Cow::from("Content-Type"), content_type.into()));

        Self { contents, headers }
    }

    /// Create a new raw MIME part that includes both headers and body.
    pub fn raw(contents: impl Into<BodyPart<'x>>) -> Self {
        Self {
            contents: contents.into(),
            headers: vec![],
        }
    }

    /// Set the attachment filename of a MIME part.
    pub fn attachment(mut self, filename: impl Into<Cow<'x, str>>) -> Self {
        self.headers.push((
            "Content-Disposition".into(),
            ContentType::new("attachment")
                .attribute("filename", filename)
                .into(),
        ));
        self
    }

    /// Set the MIME part as inline.
    pub fn inline(mut self) -> Self {
        self.headers.push((
            "Content-Disposition".into(),
            ContentType::new("inline").into(),
        ));
        self
    }

    /// Set the Content-Language header of a MIME part.
    pub fn language(mut self, value: impl Into<Cow<'x, str>>) -> Self {
        self.headers
            .push(("Content-Language".into(), Text::new(value).into()));
        self
    }

    /// Set the Content-ID header of a MIME part.
    pub fn cid(mut self, value: impl Into<Cow<'x, str>>) -> Self {
        self.headers
            .push(("Content-ID".into(), MessageId::new(value).into()));
        self
    }

    /// Set the Content-Location header of a MIME part.
    pub fn location(mut self, value: impl Into<Cow<'x, str>>) -> Self {
        self.headers
            .push(("Content-Location".into(), Raw::new(value).into()));
        self
    }

    /// Disable automatic Content-Transfer-Encoding detection and treat this as a raw MIME part
    pub fn transfer_encoding(mut self, value: impl Into<Cow<'x, str>>) -> Self {
        self.headers
            .push(("Content-Transfer-Encoding".into(), Raw::new(value).into()));
        self
    }

    /// Set custom headers of a MIME part.
    pub fn header(
        mut self,
        header: impl Into<Cow<'x, str>>,
        value: impl Into<HeaderType<'x>>,
    ) -> Self {
        self.headers.push((header.into(), value.into()));
        self
    }

    /// Returns the part's size
    pub fn size(&self) -> usize {
        match &self.contents {
            BodyPart::Text(b) => b.len(),
            BodyPart::Binary(b) => b.len(),
            BodyPart::Multipart(bl) => bl.iter().map(|b| b.size()).sum(),
        }
    }

    pub(crate) fn estimated_len(&self) -> usize {
        self.estimated_len_at_depth(0)
    }

    fn estimated_len_at_depth(&self, depth: usize) -> usize {
        let headers = estimated_headers_len(&self.headers);
        match &self.contents {
            BodyPart::Text(text) => headers
                .saturating_add(base64_len(text.len()))
                .saturating_add(LEAF_OVERHEAD),
            BodyPart::Binary(binary) => headers
                .saturating_add(base64_len(binary.len()))
                .saturating_add(LEAF_OVERHEAD),
            BodyPart::Multipart(parts) if depth < MAX_ESTIMATE_DEPTH => {
                parts
                    .iter()
                    .fold(headers.saturating_add(MULTIPART_OVERHEAD), |total, part| {
                        total
                            .saturating_add(part.estimated_len_at_depth(depth + 1))
                            .saturating_add(BOUNDARY_OVERHEAD)
                    })
            }
            BodyPart::Multipart(parts) => headers
                .saturating_add(MULTIPART_OVERHEAD)
                .saturating_add(parts.len().saturating_mul(BOUNDARY_OVERHEAD)),
        }
    }

    /// Add a body part to a multipart/* MIME part.
    pub fn add_part(&mut self, part: MimePart<'x>) {
        if let BodyPart::Multipart(ref mut parts) = self.contents {
            parts.push(part);
        }
    }

    /// Write the MIME part to a writer.
    pub fn write_part(self, output: &mut impl Writer) {
        let children = match self.contents {
            BodyPart::Text(text) => {
                return write_leaf_part(&self.headers, text.as_bytes(), true, output);
            }
            BodyPart::Binary(binary) => {
                return write_leaf_part(&self.headers, binary.as_ref(), false, output);
            }
            BodyPart::Multipart(children) => children,
        };

        let mut boundary = write_multipart_headers(self.headers, output);
        let mut parts = children.into_iter();
        let mut stack: Vec<(std::vec::IntoIter<MimePart<'x>>, Cow<'x, str>)> = Vec::new();

        loop {
            while let Some(part) = parts.next() {
                output.write(b"\r\n--");
                output.write(boundary.as_bytes());
                output.write(b"\r\n");

                match part.contents {
                    BodyPart::Text(text) => {
                        write_leaf_part(&part.headers, text.as_bytes(), true, output);
                    }
                    BodyPart::Binary(binary) => {
                        write_leaf_part(&part.headers, binary.as_ref(), false, output);
                    }
                    BodyPart::Multipart(children) => {
                        stack.push((parts, boundary));
                        boundary = write_multipart_headers(part.headers, output);
                        parts = children.into_iter();
                    }
                }
            }

            output.write(b"\r\n--");
            output.write(boundary.as_bytes());
            output.write(b"--\r\n");

            match stack.pop() {
                Some((previous_parts, previous_boundary)) => {
                    parts = previous_parts;
                    boundary = previous_boundary;
                }
                None => return,
            }
        }
    }
}

const MAX_ESTIMATE_DEPTH: usize = 32;
const LEAF_OVERHEAD: usize = 96;
const MULTIPART_OVERHEAD: usize = 128;
const BOUNDARY_OVERHEAD: usize = 64;

#[inline(always)]
pub(crate) fn base64_len(len: usize) -> usize {
    let encoded = len.div_ceil(3).saturating_mul(4);
    encoded.saturating_add(encoded.div_ceil(76).saturating_mul(2))
}

pub(crate) fn estimated_headers_len(headers: &[(Cow<'_, str>, HeaderType<'_>)]) -> usize {
    headers
        .iter()
        .map(|(name, _)| name.len() + HEADER_VALUE_ESTIMATE)
        .sum()
}

const HEADER_VALUE_ESTIMATE: usize = 64;

fn write_leaf_part(
    headers: &[(Cow<'_, str>, HeaderType<'_>)],
    body: &[u8],
    is_text_body: bool,
    output: &mut impl Writer,
) {
    let mut is_text = is_text_body;
    let mut is_attachment = false;
    let mut is_raw = headers.is_empty();

    for (header_name, header_value) in headers {
        write_header_name(header_name, output);

        if !is_text && header_name == "Content-Type" {
            is_text = header_value
                .as_content_type()
                .is_some_and(|value| value.is_text());
        } else if !is_attachment && header_name == "Content-Disposition" {
            is_attachment = header_value
                .as_content_type()
                .is_some_and(|value| value.is_attachment());
        } else if !is_raw && header_name == "Content-Transfer-Encoding" {
            is_raw = true;
        }

        header_value.write_header(output, header_name.len() + 2);
    }

    if !is_raw {
        if is_text {
            write_encoded_body(body, output, !is_attachment);
        } else {
            output.write(b"Content-Transfer-Encoding: base64\r\n\r\n");
            base64_encode_wrapped(body, output);
        }
    } else {
        if !headers.is_empty() {
            output.write(b"\r\n");
        }
        output.write(body);
    }
}

fn write_multipart_headers<'x>(
    headers: Vec<(Cow<'x, str>, HeaderType<'x>)>,
    output: &mut impl Writer,
) -> Cow<'x, str> {
    let mut boundary: Option<Cow<'x, str>> = None;

    for (header_name, header_value) in headers {
        write_header_name(&header_name, output);

        if boundary.is_none() && header_name.eq_ignore_ascii_case("Content-Type") {
            boundary = Some(match header_value {
                HeaderType::ContentType(mut content_type) => {
                    let position = match content_type
                        .attributes
                        .iter()
                        .position(|(attribute, _)| attribute.eq_ignore_ascii_case("boundary"))
                    {
                        Some(position) => position,
                        None => {
                            let position = content_type.attributes.len();
                            content_type
                                .attributes
                                .push(("boundary".into(), make_boundary("_").into()));
                            position
                        }
                    };
                    content_type.write_header(output, 14);
                    content_type.attributes.swap_remove(position).1
                }
                HeaderType::Raw(raw) => raw_boundary(raw, output),
                HeaderType::Text(text) => raw_boundary(Raw::new(text.text), output),
                other => {
                    other.write_header(output, header_name.len() + 2);
                    continue;
                }
            });
        } else {
            header_value.write_header(output, header_name.len() + 2);
        }
    }

    let boundary = boundary.unwrap_or_else(|| {
        output.write(b"Content-Type: ");
        let boundary = make_boundary("_");
        ContentType::new("multipart/mixed")
            .attribute("boundary", &boundary)
            .write_header(output, 14);
        boundary.into()
    });

    output.write(b"\r\n");
    boundary
}

fn raw_boundary<'x>(raw: Raw<'x>, output: &mut impl Writer) -> Cow<'x, str> {
    match raw.raw.find("boundary=\"") {
        Some(position) => {
            raw.write_header(output, 14);
            match raw.raw {
                Cow::Borrowed(value) => value
                    .get(position..)
                    .and_then(|tail| tail.split('"').nth(1))
                    .map_or_else(|| make_boundary("_").into(), Cow::Borrowed),
                Cow::Owned(value) => value
                    .get(position..)
                    .and_then(|tail| tail.split('"').nth(1))
                    .map_or_else(|| make_boundary("_"), str::to_string)
                    .into(),
            }
        }
        None => {
            let boundary = make_boundary("_");
            output.write(raw.raw.as_bytes());
            output.write(b"; boundary=\"");
            output.write(boundary.as_bytes());
            output.write(b"\"\r\n");
            boundary.into()
        }
    }
}

#[inline(always)]
pub(crate) fn write_header_name(name: &str, output: &mut impl Writer) {
    output.write(name.as_bytes());
    output.write(b": ");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unexpected_multipart_content_type_values_do_not_panic() {
        let mut output = Vec::new();
        MimePart::raw(vec![MimePart::new("text/plain", "hello")])
            .header("Content-Type", Text::new("multipart/mixed"))
            .write_part(&mut output);
        let output = String::from_utf8(output).unwrap();
        assert!(
            output.starts_with("Content-Type: multipart/mixed; boundary=\""),
            "{output:?}"
        );
        assert!(output.ends_with("--\r\n"), "{output:?}");

        let mut output = Vec::new();
        MimePart::raw(vec![MimePart::new("text/plain", "hello")])
            .header("Content-Type", MessageId::new("id@example.org"))
            .write_part(&mut output);
        let output = String::from_utf8(output).unwrap();
        assert!(
            output.contains("Content-Type: multipart/mixed;") && output.contains("boundary=\""),
            "{output:?}"
        );
    }

    #[test]
    fn raw_content_type_with_boundary_is_written_and_reused() {
        let mut output = Vec::new();
        MimePart::raw(vec![MimePart::new("text/plain", "hello")])
            .header(
                "Content-Type",
                Raw::new("multipart/mixed; boundary=\"abc\""),
            )
            .write_part(&mut output);
        let output = String::from_utf8(output).unwrap();
        assert!(
            output.starts_with(
                "Content-Type: multipart/mixed; boundary=\"abc\"\r\n\r\n\r\n--abc\r\n"
            ),
            "{output:?}"
        );
        assert!(output.ends_with("\r\n--abc--\r\n"), "{output:?}");
    }
    use std::collections::HashSet;

    fn is_boundary_safe(value: &str) -> bool {
        value.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'.' | b'-' | b'+' | b'=' | b':')
        })
    }

    #[test]
    fn hex_digits_match_the_formatter() {
        for value in [
            0u64,
            1,
            9,
            10,
            15,
            16,
            255,
            256,
            0xFFFF_FFFF,
            0x1_0000_0000,
            u64::MAX,
            u64::MAX - 1,
            0x0123_4567_89AB_CDEF,
        ] {
            let mut buffer = [0u8; BOUNDARY_HEX_LEN];
            let at = push_hex(&mut buffer, BOUNDARY_HEX_LEN, value);
            let digits = std::str::from_utf8(buffer.get(at..).unwrap_or_default()).unwrap();
            assert_eq!(digits, format!("{value:x}"), "value {value}");
        }
    }

    #[test]
    fn boundary_keeps_its_shape() {
        let boundary = make_boundary("_");
        let fields = boundary.split('_').collect::<Vec<_>>();
        assert_eq!(fields.len(), 3, "{boundary}");
        assert!(
            fields.iter().all(
                |field| !field.is_empty() && field.bytes().all(|byte| byte.is_ascii_hexdigit())
            ),
            "{boundary}"
        );
        assert!(boundary.len() <= 70, "{boundary}");
        assert!(is_boundary_safe(&boundary), "{boundary}");
        let dotted = make_boundary(".");
        assert_eq!(dotted.split('.').count(), 3, "{dotted}");
        assert!(is_boundary_safe(&dotted), "{dotted}");
        let long = make_boundary("_separator_");
        assert_eq!(long.split("_separator_").count(), 3, "{long}");
    }

    #[test]
    fn boundaries_are_unique_within_a_thread() {
        let count = 1_000_000;
        let mut seen = HashSet::with_capacity(count);
        for _ in 0..count {
            let boundary = make_boundary("_");
            assert!(is_boundary_safe(&boundary), "{boundary}");
            assert!(seen.insert(boundary), "duplicate boundary");
        }
        assert_eq!(seen.len(), count);
    }

    #[test]
    fn boundaries_are_unique_across_threads() {
        let per_thread = 50_000;
        let threads = (0..8)
            .map(|_| {
                std::thread::spawn(move || {
                    (0..per_thread)
                        .map(|_| make_boundary("_"))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let mut seen = HashSet::with_capacity(per_thread * 8);
        for thread in threads {
            for boundary in thread.join().expect("thread panicked") {
                assert!(is_boundary_safe(&boundary), "{boundary}");
                assert!(seen.insert(boundary), "duplicate boundary");
            }
        }
        assert_eq!(seen.len(), per_thread * 8);
    }

    #[test]
    fn message_id_boundaries_are_unique_across_threads() {
        let per_thread = 20_000;
        let threads = (0..8)
            .map(|_| {
                std::thread::spawn(move || {
                    (0..per_thread)
                        .map(|_| make_boundary("."))
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();
        let mut seen = HashSet::with_capacity(per_thread * 8);
        for thread in threads {
            for boundary in thread.join().expect("thread panicked") {
                assert!(seen.insert(boundary), "duplicate boundary");
            }
        }
        assert_eq!(seen.len(), per_thread * 8);
    }

    #[test]
    fn estimates_cover_the_output() {
        let ascii = "lorem ipsum dolor sit amet ".repeat(40_000);
        let latin = "réunion d'équipe: résumé ".repeat(40_000);
        let cjk = "안녕하세요 세계 ".repeat(40_000);
        let binary: Vec<u8> = (0..1_000_000u32).map(|value| value as u8).collect();
        let parts = vec![
            MimePart::new("text/plain", ascii.as_str()),
            MimePart::new("text/plain", latin.as_str()),
            MimePart::new("text/plain", cjk.as_str()),
            MimePart::new("image/png", binary.as_slice()),
            MimePart::new("text/plain", cjk.as_str()).attachment("report.txt"),
            MimePart::new("image/png", binary.as_slice()).attachment("photo.png"),
            MimePart::new(
                "multipart/alternative",
                vec![
                    MimePart::new("text/plain", "short"),
                    MimePart::new("text/html", "<p>short</p>"),
                ],
            ),
        ];
        for part in parts {
            let estimate = part.estimated_len();
            let mut output = Vec::new();
            part.write_part(&mut output);
            assert!(
                estimate >= output.len(),
                "estimate {estimate} below output {}",
                output.len()
            );
            assert!(
                estimate <= output.len() * 2,
                "estimate {estimate} more than twice the output {}",
                output.len()
            );
        }
    }
}
