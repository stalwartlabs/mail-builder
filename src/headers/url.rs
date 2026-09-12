/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use super::{Header, fold::FoldWriter};
use crate::writer::Writer;
use std::borrow::Cow;

/// URL header, used mostly on List-* headers
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct URL<'x> {
    pub url: Vec<Cow<'x, str>>,
}

impl<'x> URL<'x> {
    /// Create a new URL header
    pub fn new(url: impl Into<Cow<'x, str>>) -> Self {
        Self {
            url: vec![url.into()],
        }
    }

    /// Create a new multi-value URL header
    pub fn new_list<T, U>(urls: T) -> Self
    where
        T: Iterator<Item = U>,
        U: Into<Cow<'x, str>>,
    {
        Self {
            url: urls.map(|s| s.into()).collect(),
        }
    }
}

impl<'x> From<&'x str> for URL<'x> {
    fn from(value: &'x str) -> Self {
        Self::new(value)
    }
}

impl From<String> for URL<'_> {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl<'x> From<&[&'x str]> for URL<'x> {
    fn from(value: &[&'x str]) -> Self {
        URL {
            url: value.iter().map(|&s| s.into()).collect(),
        }
    }
}

impl<'x> From<&'x [String]> for URL<'x> {
    fn from(value: &'x [String]) -> Self {
        URL {
            url: value.iter().map(|s| s.into()).collect(),
        }
    }
}

impl<'x, T> From<Vec<T>> for URL<'x>
where
    T: Into<Cow<'x, str>>,
{
    fn from(value: Vec<T>) -> Self {
        URL {
            url: value.into_iter().map(|s| s.into()).collect(),
        }
    }
}

impl Header for URL<'_> {
    fn write_header(&self, output: &mut impl Writer, column: usize) {
        let mut folder = FoldWriter::new(output, column);

        if let Some((last, head)) = self.url.split_last() {
            for url in head {
                folder.begin_atom(url.len() + 3);
                folder.write_byte(b'<');
                folder.write(url.as_bytes());
                folder.write(b">,");
                folder.space();
            }
            folder.begin_atom(last.len() + 2);
            folder.write_byte(b'<');
            folder.write(last.as_bytes());
            folder.write(b">\r\n");
            return;
        }

        folder.finish();
    }
}
