pub mod base64;
pub mod encode;
pub mod quoted_printable;

use std::{borrow::Cow, collections::HashMap};

pub struct MessageBuilder<'x> {
    pub headers: HashMap<String, Vec<Cow<'x, str>>>,
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

pub struct EmailAddress<'x> {
    pub name: Option<Cow<'x, str>>,
    pub email: Cow<'x, str>,
}

pub struct GroupedAddresses<'x> {
    pub name: Option<Cow<'x, str>>,
    pub addresses: Vec<EmailAddress<'x>>,
}

pub enum Address<'x> {
    Address(EmailAddress<'x>),
    Group(GroupedAddresses<'x>),
    List(Vec<Address<'x>>),
}

pub struct MessageIds<'x> {
    pub id: Vec<Cow<'x, str>>,
}

pub struct Date<'x> {
    pub date: Cow<'x, str>,
}

pub struct URL<'x> {
    pub url: Vec<Cow<'x, str>>,
}

pub struct UnstructuredText<'x> {
    pub text: Cow<'x, str>,
}

pub struct Raw<'x> {
    pub raw: Cow<'x, str>,
}

pub trait RawHeader {
    fn into_raw_header(self) -> String;
}

impl<'x> RawHeader for Address<'x> {
    fn into_raw_header(self) -> String {
        todo!()
    }
}

impl<'x> RawHeader for MessageIds<'x> {
    fn into_raw_header(self) -> String {
        todo!()
    }
}

impl<'x> RawHeader for Date<'x> {
    fn into_raw_header(self) -> String {
        todo!()
    }
}

impl<'x> RawHeader for URL<'x> {
    fn into_raw_header(self) -> String {
        todo!()
    }
}

impl<'x> RawHeader for UnstructuredText<'x> {
    fn into_raw_header(self) -> String {
        todo!()
    }
}

impl<'x> RawHeader for Raw<'x> {
    fn into_raw_header(self) -> String {
        self.raw.into_owned()
    }
}

impl<'x> MessageBuilder<'x> {
    pub fn message_id(&mut self, value: MessageIds) {
        self.header("Message-ID", value);
    }

    pub fn in_reply_to(&mut self, value: MessageIds) {
        self.header("In-Reply-To", value);
    }

    pub fn references(&mut self, value: MessageIds) {
        self.header("References", value);
    }

    pub fn sender(&mut self, value: Address) {
        self.header("Sender", value);
    }

    pub fn from(&mut self, value: Address) {
        self.header("From", value);
    }

    pub fn to(&mut self, value: Address) {
        self.header("To", value);
    }

    pub fn cc(&mut self, value: Address) {
        self.header("Cc", value);
    }

    pub fn bcc(&mut self, value: Address) {
        self.header("Bcc", value);
    }

    pub fn reply_to(&mut self, value: Address) {
        self.header("Reply-To", value);
    }

    pub fn subject(&mut self, value: Cow<'x, str>) {
        self.header("From", UnstructuredText { text: value });
    }

    pub fn date(&mut self, value: Date) {
        self.header("Date", value);
    }

    pub fn header(&mut self, header: &str, value: impl RawHeader) {
        self.headers
            .entry(header.to_string())
            .or_insert_with(Vec::new)
            .push(value.into_raw_header().into());
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
