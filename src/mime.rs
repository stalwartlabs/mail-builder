use std::{
    borrow::Cow,
    collections::{
        hash_map::{DefaultHasher, Entry},
        HashMap,
    },
    hash::{Hash, Hasher},
    io::{self, Write},
    iter::FromIterator,
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

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

pub struct MimePart<'x> {
    pub headers: HashMap<String, HeaderType<'x>>,
    pub contents: BodyPart<'x>,
}

pub enum BodyPart<'x> {
    Text(&'x str),
    Binary(&'x [u8]),
    Multipart(Vec<MimePart<'x>>),
}

impl<'x> From<&'x str> for BodyPart<'x> {
    fn from(value: &'x str) -> Self {
        BodyPart::Text(value)
    }
}

impl<'x> From<&'x [u8]> for BodyPart<'x> {
    fn from(value: &'x [u8]) -> Self {
        BodyPart::Binary(value)
    }
}

pub fn make_boundary() -> String {
    let mut s = DefaultHasher::new();
    gethostname::gethostname().hash(&mut s);
    format!(
        "stlwrt_{}_{:?}_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::new(0, 0))
            .as_nanos(),
        thread::current().id(),
        s.finish()
    )
}

impl<'x> MimePart<'x> {
    pub fn new(ctype: ContentType<'x>, contents: BodyPart<'x>) -> Self {
        Self {
            contents,
            headers: HashMap::from_iter(vec![("Content-Type".into(), ctype.into())]),
        }
    }

    pub fn new_multipart(ctype: &'x str, contents: Vec<MimePart<'x>>) -> Self {
        Self {
            contents: BodyPart::Multipart(contents),
            headers: HashMap::from_iter(vec![(
                "Content-Type".to_string(),
                ContentType::new(ctype).into(),
            )]),
        }
    }

    pub fn new_text(contents: &'x str) -> Self {
        Self {
            contents: contents.into(),
            headers: HashMap::from_iter(vec![(
                "Content-Type".to_string(),
                ContentType::new("text/plain")
                    .attribute("charset", "utf-8")
                    .into(),
            )]),
        }
    }

    pub fn new_html(contents: &'x str) -> Self {
        Self {
            contents: contents.into(),
            headers: HashMap::from_iter(vec![(
                "Content-Type".to_string(),
                ContentType::new("text/html")
                    .attribute("charset", "utf-8")
                    .into(),
            )]),
        }
    }

    pub fn new_binary(c_type: &'x str, contents: &'x [u8]) -> Self {
        Self {
            contents: contents.into(),
            headers: HashMap::from_iter(vec![(
                "Content-Type".to_string(),
                ContentType::new(c_type)
                    .attribute("charset", "utf-8")
                    .into(),
            )]),
        }
    }

    pub fn attachment(mut self, filename: &'x str) -> Self {
        self.headers.insert(
            "Content-Disposition".to_string(),
            ContentType::new("attachment")
                .attribute("filename", filename)
                .into(),
        );
        self
    }

    pub fn inline(mut self) -> Self {
        self.headers.insert(
            "Content-Disposition".to_string(),
            ContentType::new("inline").into(),
        );
        self
    }

    pub fn language(mut self, value: &'x str) -> Self {
        self.headers
            .insert("Content-Language".to_string(), Text::new(value).into());
        self
    }

    pub fn cid(mut self, value: &'x str) -> Self {
        self.headers
            .insert("Content-ID".to_string(), MessageId::new(value).into());
        self
    }

    pub fn location(mut self, value: &'x str) -> Self {
        self.headers
            .insert("Content-Location".to_string(), Raw::new(value).into());
        self
    }

    pub fn header(mut self, header: &str, value: HeaderType<'x>) -> Self {
        self.headers.insert(header.to_string(), value);
        self
    }

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
                        match get_encoding_type(text, false) {
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
                                output.write_all(b"\r\n")?;
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
                        base64_encode(binary, &mut output, false)?;
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
                                        content_type.attributes.entry("boundary")
                                    {
                                        entry.insert(boundary.as_ref().unwrap());
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
