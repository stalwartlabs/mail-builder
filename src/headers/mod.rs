/*
 * Copyright Stalwart Labs Ltd. See the COPYING
 * file at the top-level directory of this distribution.
 *
 * Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
 * https://www.apache.org/licenses/LICENSE-2.0> or the MIT license
 * <LICENSE-MIT or https://opensource.org/licenses/MIT>, at your
 * option. This file may not be copied, modified, or distributed
 * except according to those terms.
 */

pub mod address;
pub mod content_type;
pub mod date;
pub mod message_id;
pub mod raw;
pub mod text;
pub mod url;

use std::io::{self, Write};

use self::{
    address::Address, content_type::ContentType, date::Date, message_id::MessageId, raw::Raw,
    text::Text, url::URL,
};

pub trait Header {
    fn write_header(&self, output: impl Write, bytes_written: usize) -> io::Result<usize>;
}

pub enum HeaderType {
    Address(Address),
    Date(Date),
    MessageId(MessageId),
    Raw(Raw),
    Text(Text),
    URL(URL),
    ContentType(ContentType),
}

impl From<Address> for HeaderType {
    fn from(value: Address) -> Self {
        HeaderType::Address(value)
    }
}

impl From<ContentType> for HeaderType {
    fn from(value: ContentType) -> Self {
        HeaderType::ContentType(value)
    }
}

impl From<Date> for HeaderType {
    fn from(value: Date) -> Self {
        HeaderType::Date(value)
    }
}
impl From<MessageId> for HeaderType {
    fn from(value: MessageId) -> Self {
        HeaderType::MessageId(value)
    }
}
impl From<Raw> for HeaderType {
    fn from(value: Raw) -> Self {
        HeaderType::Raw(value)
    }
}
impl From<Text> for HeaderType {
    fn from(value: Text) -> Self {
        HeaderType::Text(value)
    }
}

impl From<URL> for HeaderType {
    fn from(value: URL) -> Self {
        HeaderType::URL(value)
    }
}

impl Header for HeaderType {
    fn write_header(&self, output: impl Write, bytes_written: usize) -> io::Result<usize> {
        match self {
            HeaderType::Address(value) => value.write_header(output, bytes_written),
            HeaderType::Date(value) => value.write_header(output, bytes_written),
            HeaderType::MessageId(value) => value.write_header(output, bytes_written),
            HeaderType::Raw(value) => value.write_header(output, bytes_written),
            HeaderType::Text(value) => value.write_header(output, bytes_written),
            HeaderType::URL(value) => value.write_header(output, bytes_written),
            HeaderType::ContentType(value) => value.write_header(output, bytes_written),
        }
    }
}

impl HeaderType {
    pub fn as_content_type(&self) -> Option<&ContentType> {
        match self {
            HeaderType::ContentType(value) => Some(value),
            _ => None,
        }
    }
}
