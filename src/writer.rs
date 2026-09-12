/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use std::io::{self, Write};

/// Byte sink used by every serializer in this crate.
///
/// `Vec<u8>` is the primary implementation; [`IoWriter`] adapts any
/// [`std::io::Write`] with an internal buffer.
pub trait Writer {
    fn write(&mut self, bytes: &[u8]);

    #[inline]
    fn write_byte(&mut self, byte: u8) {
        self.write(&[byte]);
    }

    #[inline]
    fn reserve(&mut self, _additional: usize) {}

    /// Hands the sink a scratch region of `len` bytes, keeps the first
    /// `fill(region)` bytes of it and discards the rest.
    fn write_with(&mut self, len: usize, fill: impl FnOnce(&mut [u8]) -> usize) {
        let mut buffer = vec![0; len];
        let written = fill(&mut buffer).min(len);
        self.write(buffer.get(..written).unwrap_or_default());
    }
}

impl Writer for Vec<u8> {
    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }

    #[inline(always)]
    fn write_byte(&mut self, byte: u8) {
        self.push(byte);
    }

    #[inline]
    fn reserve(&mut self, additional: usize) {
        Vec::reserve(self, additional);
    }

    #[inline]
    fn write_with(&mut self, len: usize, fill: impl FnOnce(&mut [u8]) -> usize) {
        let start = self.len();
        self.resize(start + len, 0);
        let written = fill(self.get_mut(start..).unwrap_or_default()).min(len);
        self.truncate(start + written);
    }
}

impl<W: Writer + ?Sized> Writer for &mut W {
    #[inline(always)]
    fn write(&mut self, bytes: &[u8]) {
        (**self).write(bytes);
    }

    #[inline(always)]
    fn write_byte(&mut self, byte: u8) {
        (**self).write_byte(byte);
    }

    #[inline]
    fn reserve(&mut self, additional: usize) {
        (**self).reserve(additional);
    }

    #[inline]
    fn write_with(&mut self, len: usize, fill: impl FnOnce(&mut [u8]) -> usize) {
        (**self).write_with(len, fill);
    }
}

pub(crate) const IO_BUFFER_MAX: usize = 64 * 1024;
pub(crate) const IO_BUFFER_MIN: usize = 4 * 1024;

/// Buffered adapter that lets any [`std::io::Write`] act as a [`Writer`].
///
/// Errors are sticky: the first failure stops all further output and is
/// returned by [`IoWriter::finish`]. Buffered bytes are only written by
/// [`IoWriter::finish`] or [`IoWriter::into_result`]; dropping the adapter
/// discards them.
pub struct IoWriter<W: Write> {
    inner: W,
    buffer: Vec<u8>,
    error: Option<io::Error>,
}

impl<W: Write> IoWriter<W> {
    pub fn new(inner: W) -> Self {
        Self::with_capacity(IO_BUFFER_MAX, inner)
    }

    pub fn with_capacity(capacity: usize, inner: W) -> Self {
        IoWriter {
            inner,
            buffer: Vec::with_capacity(capacity.max(1)),
            error: None,
        }
    }

    fn write_inner(&mut self, bytes: &[u8]) {
        if self.error.is_none()
            && let Err(err) = self.inner.write_all(bytes)
        {
            self.error = Some(err);
        }
    }

    fn flush_buffer(&mut self) {
        if !self.buffer.is_empty() {
            let buffer = std::mem::take(&mut self.buffer);
            self.write_inner(&buffer);
            self.buffer = buffer;
            self.buffer.clear();
        }
    }

    /// Flushes the buffer and returns the wrapped writer, or the first
    /// error that occurred.
    pub fn finish(mut self) -> io::Result<W> {
        self.flush_buffer();
        match self.error.take() {
            Some(err) => Err(err),
            None => Ok(self.inner),
        }
    }

    pub fn into_result(self) -> io::Result<()> {
        self.finish().map(|_| ())
    }
}

impl<W: Write> Writer for IoWriter<W> {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        if bytes.len() > self.buffer.capacity() - self.buffer.len() {
            self.flush_buffer();
            if bytes.len() >= self.buffer.capacity() {
                self.write_inner(bytes);
                return;
            }
        }
        self.buffer.extend_from_slice(bytes);
    }

    #[inline]
    fn write_byte(&mut self, byte: u8) {
        if self.buffer.len() == self.buffer.capacity() {
            self.flush_buffer();
        }
        self.buffer.push(byte);
    }

    #[inline]
    fn write_with(&mut self, len: usize, fill: impl FnOnce(&mut [u8]) -> usize) {
        if len > self.buffer.capacity() - self.buffer.len() {
            self.flush_buffer();
        }
        let start = self.buffer.len();
        self.buffer.resize(start + len, 0);
        let written = fill(self.buffer.get_mut(start..).unwrap_or_default()).min(len);
        self.buffer.truncate(start + written);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FailAfter(usize);

    impl Write for FailAfter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            if self.0 == 0 {
                Err(io::Error::other("full"))
            } else {
                self.0 = self.0.saturating_sub(buf.len());
                Ok(buf.len())
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[derive(Default)]
    struct Chunks {
        writes: Vec<Vec<u8>>,
    }

    impl Write for Chunks {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.writes.push(buf.to_vec());
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    impl Chunks {
        fn joined(&self) -> Vec<u8> {
            self.writes.iter().flatten().copied().collect()
        }
    }

    #[test]
    fn io_writer_buffers_and_flushes_in_order() {
        let mut writer = IoWriter::with_capacity(8, Vec::new());
        writer.write(b"abc");
        writer.write_byte(b'd');
        writer.write(b"efghij");
        writer.write_with(4, |region| {
            region.copy_from_slice(b"KLMN");
            2
        });
        writer.write(b"0123456789abcdef");
        assert_eq!(writer.finish().unwrap(), b"abcdefghijKL0123456789abcdef");
    }

    #[test]
    fn io_writer_reports_the_first_error() {
        let mut writer = IoWriter::with_capacity(4, FailAfter(6));
        writer.write(b"abcdef");
        writer.write(b"ghij");
        writer.write(b"klmn");
        assert!(writer.into_result().is_err());
    }

    #[test]
    fn io_writer_keeps_writing_after_the_first_error() {
        let mut writer = IoWriter::with_capacity(4, FailAfter(0));
        for _ in 0..100 {
            writer.write(b"abcdefgh");
            writer.write_byte(b'x');
        }
        let error = writer.into_result().expect_err("error expected");
        assert_eq!(error.to_string(), "full");
    }

    #[test]
    fn io_writer_passes_large_writes_through() {
        let mut writer = IoWriter::with_capacity(8, Chunks::default());
        writer.write(b"ab");
        writer.write(b"0123456789");
        writer.write(b"cd");
        let chunks = writer.finish().unwrap();
        assert_eq!(chunks.writes.len(), 3);
        assert_eq!(
            chunks.writes.first().map(Vec::as_slice),
            Some(b"ab".as_ref())
        );
        assert_eq!(
            chunks.writes.get(1).map(Vec::as_slice),
            Some(b"0123456789".as_ref())
        );
        assert_eq!(chunks.joined(), b"ab0123456789cd");
    }

    #[test]
    fn io_writer_byte_writes_never_exceed_the_buffer() {
        let mut writer = IoWriter::with_capacity(4, Chunks::default());
        for byte in b"abcdefghij" {
            writer.write_byte(*byte);
        }
        let chunks = writer.finish().unwrap();
        assert!(
            chunks.writes.iter().all(|chunk| chunk.len() <= 4),
            "{:?}",
            chunks.writes
        );
        assert_eq!(chunks.joined(), b"abcdefghij");
    }

    #[test]
    fn io_writer_write_with_grows_beyond_the_buffer() {
        let mut writer = IoWriter::with_capacity(4, Chunks::default());
        writer.write(b"ab");
        writer.write_with(16, |region| {
            for (slot, byte) in region.iter_mut().zip(b"0123456789".iter()) {
                *slot = *byte;
            }
            10
        });
        writer.write(b"cd");
        let chunks = writer.finish().unwrap();
        assert_eq!(chunks.joined(), b"ab0123456789cd");
    }

    #[test]
    fn io_writer_write_with_keeps_only_the_filled_prefix() {
        let mut writer = IoWriter::with_capacity(64, Vec::new());
        writer.write_with(8, |region| {
            assert_eq!(region.len(), 8);
            assert!(region.iter().all(|byte| *byte == 0));
            region.iter_mut().for_each(|slot| *slot = b'z');
            3
        });
        writer.write_with(8, |_| 0);
        writer.write(b"!");
        assert_eq!(writer.finish().unwrap(), b"zzz!");
    }

    #[test]
    fn io_writer_write_with_caps_the_reported_length() {
        let mut writer = IoWriter::with_capacity(64, Vec::new());
        writer.write_with(4, |region| {
            region.copy_from_slice(b"abcd");
            usize::MAX
        });
        assert_eq!(writer.finish().unwrap(), b"abcd");
    }

    #[test]
    fn vec_write_with_keeps_only_the_filled_prefix() {
        let mut out = b"x".to_vec();
        out.write_with(6, |region| {
            region[..3].copy_from_slice(b"abc");
            3
        });
        assert_eq!(out, b"xabc");
    }

    #[test]
    fn vec_write_with_zeroes_the_region() {
        let mut out = Vec::new();
        out.write_with(8, |region| {
            region.iter_mut().for_each(|slot| *slot = b'a');
            8
        });
        out.truncate(4);
        out.write_with(8, |region| {
            assert!(region.iter().all(|byte| *byte == 0));
            0
        });
        assert_eq!(out, b"aaaa");
    }

    #[test]
    fn default_write_with_matches_the_vec_implementation() {
        struct Plain(Vec<u8>);
        impl Writer for Plain {
            fn write(&mut self, bytes: &[u8]) {
                self.0.extend_from_slice(bytes);
            }
        }

        let mut plain = Plain(Vec::new());
        plain.write_byte(b'a');
        plain.write_with(6, |region| {
            region.iter_mut().for_each(|slot| *slot = b'b');
            4
        });
        assert_eq!(plain.0, b"abbbb");
    }
}
