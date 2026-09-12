/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use super::{
    Header,
    fold::{FoldWriter, write_unstructured},
};
use crate::writer::Writer;
use std::borrow::Cow;

/// Raw e-mail header.
/// Raw headers are not encoded, only line-wrapped.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Raw<'x> {
    pub raw: Cow<'x, str>,
}

impl<'x> Raw<'x> {
    /// Create a new raw header
    pub fn new(raw: impl Into<Cow<'x, str>>) -> Self {
        Self { raw: raw.into() }
    }
}

impl<'x, T> From<T> for Raw<'x>
where
    T: Into<Cow<'x, str>>,
{
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl Header for Raw<'_> {
    fn write_header(&self, output: &mut impl Writer, column: usize) {
        let mut folder = FoldWriter::new(output, column);
        write_unstructured(&mut folder, self.raw.as_bytes());
        folder.finish();
    }
}
