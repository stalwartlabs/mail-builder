pub mod address;
pub mod date;
pub mod message_id;
pub mod raw;
pub mod text;
pub mod url;

use std::io::{self, Write};

use self::{address::Address, date::Date, message_id::MessageId, raw::Raw, text::Text, url::URL};

pub trait Header {
    fn write_header(&self, output: impl Write, line_len: usize) -> io::Result<usize>;
}

pub enum HeaderType<'x> {
    Address(Address<'x>),
    Date(Date<'x>),
    MessageId(MessageId<'x>),
    Raw(Raw<'x>),
    Text(Text<'x>),
    URL(URL<'x>),
}

impl<'x> From<Address<'x>> for HeaderType<'x> {
    fn from(value: Address<'x>) -> Self {
        HeaderType::Address(value)
    }
}

impl<'x> From<Date<'x>> for HeaderType<'x> {
    fn from(value: Date<'x>) -> Self {
        HeaderType::Date(value)
    }
}
impl<'x> From<MessageId<'x>> for HeaderType<'x> {
    fn from(value: MessageId<'x>) -> Self {
        HeaderType::MessageId(value)
    }
}
impl<'x> From<Raw<'x>> for HeaderType<'x> {
    fn from(value: Raw<'x>) -> Self {
        HeaderType::Raw(value)
    }
}
impl<'x> From<Text<'x>> for HeaderType<'x> {
    fn from(value: Text<'x>) -> Self {
        HeaderType::Text(value)
    }
}

impl<'x> From<URL<'x>> for HeaderType<'x> {
    fn from(value: URL<'x>) -> Self {
        HeaderType::URL(value)
    }
}
