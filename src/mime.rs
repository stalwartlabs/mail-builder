/*
 * Copyright Stalwart Labs, Minter Ltd. See the COPYING
 * file at the top-level directory of this distribution.
 *
 * Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
 * https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
 * <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
 * option. This file may not be copied, modified, or distributed
 * except according to those terms.
 */

use std::{
    borrow::Cow,
    collections::{
        hash_map::{DefaultHasher, Entry},
        HashMap,
    },
    hash::{Hash, Hasher},
    io::{self, Write},
    iter::FromIterator,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rand::Rng;

use crate::{
    encoders::{
        base64::base64_encode,
        encode::{get_encoding_type, EncodingType},
        quoted_printable::quoted_printable_encode,
    },
    headers::{
        content_type::ContentType, message_id::MessageId, raw::Raw, text::Text, Header, HeaderType,
    },
};

/// MIME part of an e-mail.
pub struct MimePart<'x> {
    pub headers: HashMap<Cow<'x, str>, HeaderType<'x>>,
    pub contents: BodyPart<'x>,
}

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

impl<'x> From<String> for BodyPart<'x> {
    fn from(value: String) -> Self {
        BodyPart::Text(value.into())
    }
}

impl<'x> From<Vec<u8>> for BodyPart<'x> {
    fn from(value: Vec<u8>) -> Self {
        BodyPart::Binary(value.into())
    }
}

pub fn make_boundary() -> String {
    let mut s = DefaultHasher::new();
    gethostname::gethostname().hash(&mut s);
    format!(
        "{:x}_{:x}_{:x}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::new(0, 0))
            .as_nanos(),
        rand::thread_rng().gen::<u64>(),
        s.finish()
    )
}

impl<'x> MimePart<'x> {
    /// Create a custom MIME part.
    pub fn new(content_type: ContentType<'x>, contents: BodyPart<'x>) -> Self {
        Self {
            contents,
            headers: HashMap::from_iter(vec![("Content-Type".into(), content_type.into())]),
        }
    }

    /// Create a new multipart/* MIME part.
    pub fn new_multipart(
        content_type: impl Into<Cow<'x, str>>,
        contents: Vec<MimePart<'x>>,
    ) -> Self {
        Self {
            contents: BodyPart::Multipart(contents),
            headers: HashMap::from_iter(vec![(
                "Content-Type".into(),
                ContentType::new(content_type).into(),
            )]),
        }
    }

    /// Create a new text/plain MIME part.
    pub fn new_text(contents: impl Into<Cow<'x, str>>) -> Self {
        Self {
            contents: BodyPart::Text(contents.into()),
            headers: HashMap::from_iter(vec![(
                "Content-Type".into(),
                ContentType::new("text/plain")
                    .attribute("charset", "utf-8")
                    .into(),
            )]),
        }
    }

    /// Create a new text/* MIME part.
    pub fn new_text_other(
        content_type: impl Into<Cow<'x, str>>,
        contents: impl Into<Cow<'x, str>>,
    ) -> Self {
        Self {
            contents: BodyPart::Text(contents.into()),
            headers: HashMap::from_iter(vec![(
                "Content-Type".into(),
                ContentType::new(content_type)
                    .attribute("charset", "utf-8")
                    .into(),
            )]),
        }
    }

    /// Create a new text/html MIME part.
    pub fn new_html(contents: impl Into<Cow<'x, str>>) -> Self {
        Self {
            contents: BodyPart::Text(contents.into()),
            headers: HashMap::from_iter(vec![(
                "Content-Type".into(),
                ContentType::new("text/html")
                    .attribute("charset", "utf-8")
                    .into(),
            )]),
        }
    }

    /// Create a new binary MIME part.
    pub fn new_binary(c_type: impl Into<Cow<'x, str>>, contents: impl Into<Cow<'x, [u8]>>) -> Self {
        Self {
            contents: BodyPart::Binary(contents.into()),
            headers: HashMap::from_iter(vec![(
                "Content-Type".into(),
                ContentType::new(c_type).into(),
            )]),
        }
    }

    /// Set the attachment filename of a MIME part.
    pub fn attachment(mut self, filename: impl Into<Cow<'x, str>>) -> Self {
        self.headers.insert(
            "Content-Disposition".into(),
            ContentType::new("attachment")
                .attribute("filename", filename)
                .into(),
        );
        self
    }

    /// Set the MIME part as inline.
    pub fn inline(mut self) -> Self {
        self.headers.insert(
            "Content-Disposition".into(),
            ContentType::new("inline").into(),
        );
        self
    }

    /// Set the Content-Language header of a MIME part.
    pub fn language(mut self, value: impl Into<Cow<'x, str>>) -> Self {
        self.headers
            .insert("Content-Language".into(), Text::new(value).into());
        self
    }

    /// Set the Content-ID header of a MIME part.
    pub fn cid(mut self, value: impl Into<Cow<'x, str>>) -> Self {
        self.headers
            .insert("Content-ID".into(), MessageId::new(value).into());
        self
    }

    /// Set the Content-Location header of a MIME part.
    pub fn location(mut self, value: impl Into<Cow<'x, str>>) -> Self {
        self.headers
            .insert("Content-Location".into(), Raw::new(value).into());
        self
    }

    /// Set custom headers of a MIME part.
    pub fn header(mut self, header: impl Into<Cow<'x, str>>, value: HeaderType<'x>) -> Self {
        self.headers.insert(header.into(), value);
        self
    }

    /// Write the MIME part to a writer.
    pub fn write_part(self, mut output: impl Write) -> io::Result<usize> {
        let mut stack = Vec::new();
        let mut it = vec![self].into_iter();
        let mut boundary: Option<Cow<str>> = None;

        loop {
            while let Some(part) = it.next() {
                if let Some(boundary) = boundary.as_ref() {
                    output.write_all(b"\r\n--")?;
                    output.write_all(boundary.as_bytes())?;
                    output.write_all(b"\r\n")?;
                }
                match part.contents {
                    BodyPart::Text(text) => {
                        for (header_name, header_value) in &part.headers {
                            output.write_all(header_name.as_bytes())?;
                            output.write_all(b": ")?;
                            header_value.write_header(&mut output, header_name.len() + 2)?;
                        }
                        match get_encoding_type(text.as_ref(), false) {
                            EncodingType::Base64 => {
                                output.write_all(b"Content-Transfer-Encoding: base64\r\n\r\n")?;
                                base64_encode(text.as_bytes(), &mut output, false)?;
                            }
                            EncodingType::QuotedPrintable(_) => {
                                output.write_all(
                                    b"Content-Transfer-Encoding: quoted-printable\r\n\r\n",
                                )?;
                                quoted_printable_encode(text.as_bytes(), &mut output, false)?;
                            }
                            EncodingType::None => {
                                output.write_all(b"Content-Transfer-Encoding: 7bit\r\n\r\n")?;
                                output.write_all(text.as_bytes())?;
                            }
                        }
                    }
                    BodyPart::Binary(binary) => {
                        for (header_name, header_value) in &part.headers {
                            output.write_all(header_name.as_bytes())?;
                            output.write_all(b": ")?;
                            header_value.write_header(&mut output, header_name.len() + 2)?;
                        }
                        output.write_all(b"Content-Transfer-Encoding: base64\r\n\r\n")?;
                        base64_encode(binary.as_ref(), &mut output, false)?;
                    }
                    BodyPart::Multipart(parts) => {
                        if boundary.is_some() {
                            stack.push((it, boundary));
                        }

                        boundary = Some(make_boundary().into());

                        for (header_name, mut header_value) in part.headers {
                            match &mut header_value {
                                HeaderType::ContentType(ref mut content_type)
                                    if header_name == "Content-Type" =>
                                {
                                    if let Entry::Vacant(entry) =
                                        content_type.attributes.entry("boundary".into())
                                    {
                                        entry.insert(boundary.as_ref().unwrap().as_ref().into());
                                    }
                                }
                                _ => {}
                            }
                            output.write_all(header_name.as_bytes())?;
                            output.write_all(b": ")?;
                            header_value.write_header(&mut output, header_name.len() + 2)?;
                        }
                        output.write_all(b"\r\n")?;
                        it = parts.into_iter();
                    }
                }
            }
            if let Some(boundary) = boundary {
                output.write_all(b"\r\n--")?;
                output.write_all(boundary.as_bytes())?;
                output.write_all(b"--\r\n")?;
            }
            if let Some((prev_it, prev_boundary)) = stack.pop() {
                it = prev_it;
                boundary = prev_boundary;
            } else {
                break;
            }
        }
        Ok(0)
    }
}
