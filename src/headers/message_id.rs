/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use super::{Header, fold::FoldWriter};
use crate::{mime::write_boundary, writer::Writer};
use std::borrow::Cow;

/// RFC5322 Message ID header
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct MessageId<'x> {
    pub id: Vec<Cow<'x, str>>,
}

impl<'x> MessageId<'x> {
    /// Create a new Message ID header
    pub fn new(id: impl Into<Cow<'x, str>>) -> Self {
        Self {
            id: vec![id.into()],
        }
    }

    /// Create a new multi-value Message ID header
    pub fn new_list<T, U>(ids: T) -> Self
    where
        T: Iterator<Item = U>,
        U: Into<Cow<'x, str>>,
    {
        Self {
            id: ids.map(|s| s.into()).collect(),
        }
    }
}

impl<'x> From<&'x str> for MessageId<'x> {
    fn from(value: &'x str) -> Self {
        Self::new(value)
    }
}

impl From<String> for MessageId<'_> {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl<'x> From<&[&'x str]> for MessageId<'x> {
    fn from(value: &[&'x str]) -> Self {
        MessageId {
            id: value.iter().map(|&s| s.into()).collect(),
        }
    }
}

impl<'x> From<&'x [String]> for MessageId<'x> {
    fn from(value: &'x [String]) -> Self {
        MessageId {
            id: value.iter().map(|s| s.into()).collect(),
        }
    }
}

impl<'x, T> From<Vec<T>> for MessageId<'x>
where
    T: Into<Cow<'x, str>>,
{
    fn from(value: Vec<T>) -> Self {
        MessageId {
            id: value.into_iter().map(|s| s.into()).collect(),
        }
    }
}

pub fn generate_message_id_header(output: &mut impl Writer, hostname: &str) {
    output.write_byte(b'<');
    write_boundary(output, ".");
    output.write_byte(b'@');
    output.write(hostname.as_bytes());
    output.write_byte(b'>');
}

impl Header for MessageId<'_> {
    fn write_header(&self, output: &mut impl Writer, column: usize) {
        let Some((last, head)) = self.id.split_last() else {
            output.write(b"\r\n");
            return;
        };

        let mut folder = FoldWriter::new(output, column);

        for id in head {
            folder.begin_atom(id.len() + 2);
            folder.write_byte(b'<');
            folder.write(id.as_bytes());
            folder.write_byte(b'>');
            folder.space();
        }

        folder.begin_atom(last.len() + 2);
        folder.write_byte(b'<');
        folder.write(last.as_bytes());
        folder.write(b">\r\n");
    }
}
