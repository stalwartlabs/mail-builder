pub mod encoders;
pub mod headers;

use std::{borrow::Cow, collections::HashMap};

use headers::{address::Address, date::Date, message_id::MessageId, text::Text, HeaderType};

pub struct MessageBuilder<'x> {
    pub headers: HashMap<String, Vec<HeaderType<'x>>>,
    pub body: Vec<MimePart<'x>>,
}

pub struct MimePart<'x> {
    pub headers: HashMap<String, Cow<'x, str>>,

    pub name: Option<Cow<'x, str>>,
    pub c_type: String,
    pub disposition: Option<Cow<'x, str>>,
    pub charset: Option<Cow<'x, str>>,

    pub contents: BodyPart<'x>,
}

pub enum BodyPart<'x> {
    Text(Cow<'x, str>),
    Binary(Cow<'x, [u8]>),
    Multipart(Vec<MimePart<'x>>),
}

impl<'x> MimePart<'x> {
    pub fn new_text(c_type: &str, contents: Cow<'x, str>) -> Self {
        Self {
            name: None,
            c_type: c_type.to_string(),
            disposition: None,
            charset: None,
            contents: BodyPart::Text(contents),
            headers: HashMap::new(),
        }
    }

    pub fn new_binary(c_type: &str, contents: Cow<'x, [u8]>) -> Self {
        Self {
            name: None,
            c_type: c_type.to_string(),
            disposition: None,
            charset: None,
            contents: BodyPart::Binary(contents),
            headers: HashMap::new(),
        }
    }

    pub fn new_multipart(c_type: &str, parts: Vec<MimePart<'x>>) -> Self {
        Self {
            name: None,
            c_type: c_type.to_string(),
            disposition: None,
            charset: None,
            contents: BodyPart::Multipart(parts),
            headers: HashMap::new(),
        }
    }

    pub fn name(mut self, value: Cow<'x, str>) -> Self {
        self.name = value.into();
        self
    }

    pub fn disposition(mut self, value: Cow<'x, str>) -> Self {
        self.disposition = value.into();
        self
    }

    pub fn charset(mut self, value: Cow<'x, str>) -> Self {
        self.charset = value.into();
        self
    }

    pub fn language(mut self, value: &[&str]) -> Self {
        self.headers
            .insert("Content-Language".to_string(), value.join(", ").into());
        self
    }

    pub fn cid(mut self, value: Cow<'x, str>) -> Self {
        self.headers
            .insert("Content-ID".to_string(), format!("<{}>", value).into());
        self
    }

    pub fn location(mut self, value: Cow<'x, str>) -> Self {
        self.headers.insert("Content-Location".to_string(), value);
        self
    }

    pub fn header(mut self, header: &str, value: Cow<'x, str>) -> Self {
        self.headers.insert(header.to_string(), value);
        self
    }
}

impl<'x> MessageBuilder<'x> {
    pub fn message_id(&mut self, value: MessageId<'x>) {
        self.header("Message-ID", value.into());
    }

    pub fn in_reply_to(&mut self, value: MessageId<'x>) {
        self.header("In-Reply-To", value.into());
    }

    pub fn references(&mut self, value: MessageId<'x>) {
        self.header("References", value.into());
    }

    pub fn sender(&mut self, value: Address<'x>) {
        self.header("Sender", value.into());
    }

    pub fn from(&mut self, value: Address<'x>) {
        self.header("From", value.into());
    }

    pub fn to(&mut self, value: Address<'x>) {
        self.header("To", value.into());
    }

    pub fn cc(&mut self, value: Address<'x>) {
        self.header("Cc", value.into());
    }

    pub fn bcc(&mut self, value: Address<'x>) {
        self.header("Bcc", value.into());
    }

    pub fn reply_to(&mut self, value: Address<'x>) {
        self.header("Reply-To", value.into());
    }

    pub fn subject(&mut self, value: Cow<'x, str>) {
        self.header("From", Text { text: value }.into());
    }

    pub fn date(&mut self, value: Date<'x>) {
        self.header("Date", value.into());
    }

    pub fn header(&mut self, header: &str, value: HeaderType<'x>) {
        self.headers
            .entry(header.to_string())
            .or_insert_with(Vec::new)
            .push(value);
    }

    pub fn text_body(&mut self, value: Cow<'x, str>) {
        self.body.push(MimePart::new_text("text/plain", value));
    }

    pub fn html_body(&mut self, value: Cow<'x, str>) {
        self.body.push(MimePart::new_text("text/html", value));
    }

    pub fn attachment(&mut self, content_type: &str, value: Cow<'x, [u8]>) {
        self.body.push(MimePart::new_binary(content_type, value));
    }

    pub fn body(&mut self, value: MimePart<'x>) {
        self.body.push(value);
    }
}
