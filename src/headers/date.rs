/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use std::time::SystemTime;

pub static DOW: &[&str] = &["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
pub static MONTH: &[&str] = &[
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

const DOW_BYTES: [[u8; 3]; 7] = [
    *b"Sun", *b"Mon", *b"Tue", *b"Wed", *b"Thu", *b"Fri", *b"Sat",
];
const MONTH_BYTES: [[u8; 3]; 12] = [
    *b"Jan", *b"Feb", *b"Mar", *b"Apr", *b"May", *b"Jun", *b"Jul", *b"Aug", *b"Sep", *b"Oct",
    *b"Nov", *b"Dec",
];

const RFC822_LEN: usize = 40;
const SECONDS_PER_DAY: i64 = 86400;

use super::Header;
use crate::writer::Writer;

/// RFC5322 Date header
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub date: i64,
}

#[inline(always)]
fn push<const N: usize>(buffer: &mut [u8; RFC822_LEN], pos: &mut usize, bytes: [u8; N]) {
    if let Some(slot) = buffer
        .get_mut(*pos..)
        .and_then(|tail| tail.first_chunk_mut::<N>())
    {
        *slot = bytes;
        *pos += N;
    }
}

#[inline(always)]
const fn two_digits(value: u64) -> [u8; 2] {
    [b'0' + (value / 10) as u8, b'0' + (value % 10) as u8]
}

#[inline(never)]
fn push_wide_year(buffer: &mut [u8; RFC822_LEN], pos: &mut usize, year: i64) {
    let mut digits = [0u8; 20];
    let mut value = year.unsigned_abs();
    let mut start = digits.len();

    loop {
        start -= 1;
        if let Some(slot) = digits.get_mut(start) {
            *slot = b'0' + (value % 10) as u8;
        }
        value /= 10;
        if value == 0 || start == 0 {
            break;
        }
    }

    let mut width = digits.len() - start;
    if year < 0 {
        push(buffer, pos, *b"-");
        width += 1;
    }
    while width < 4 {
        push(buffer, pos, *b"0");
        width += 1;
    }
    for &digit in digits.get(start..).unwrap_or_default() {
        push(buffer, pos, [digit]);
    }
}

impl Date {
    /// Create a new Date header from a timestamp.
    pub fn new(date: i64) -> Self {
        Self { date }
    }

    /// Create a new Date header using the current time.
    pub fn now() -> Self {
        Self {
            date: SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0) as i64,
        }
    }

    fn format_rfc822(&self, buffer: &mut [u8; RFC822_LEN]) -> usize {
        let days = self.date.div_euclid(SECONDS_PER_DAY);
        let seconds = self.date.rem_euclid(SECONDS_PER_DAY) as u64;

        let z = days + 719468;
        let era = z.div_euclid(146097);
        let doe = (z - era * 146097) as u64;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe as i64 + era * 400 + i64::from(month <= 2);

        let dow = DOW_BYTES[(days + 4).rem_euclid(7) as usize];
        let mon = MONTH_BYTES
            .get(month as usize - 1)
            .copied()
            .unwrap_or(*b"   ");

        let mut pos = 0;
        push(buffer, &mut pos, [dow[0], dow[1], dow[2], b',', b' ']);
        if day >= 10 {
            push(buffer, &mut pos, two_digits(day));
        } else {
            push(buffer, &mut pos, [b'0' + day as u8]);
        }
        push(buffer, &mut pos, [b' ', mon[0], mon[1], mon[2], b' ']);

        if (0..10000).contains(&year) {
            let year = year as u64;
            push(
                buffer,
                &mut pos,
                [
                    b'0' + (year / 1000) as u8,
                    b'0' + (year / 100 % 10) as u8,
                    b'0' + (year / 10 % 10) as u8,
                    b'0' + (year % 10) as u8,
                ],
            );
        } else {
            push_wide_year(buffer, &mut pos, year);
        }

        let [hour, minute, second] =
            [seconds / 3600, seconds / 60 % 60, seconds % 60].map(two_digits);
        push(
            buffer,
            &mut pos,
            [
                b' ', hour[0], hour[1], b':', minute[0], minute[1], b':', second[0], second[1],
                b' ',
            ],
        );
        push(buffer, &mut pos, *b"+0000");

        pos
    }

    /// Returns an RFC822 date.
    pub fn to_rfc822(&self) -> String {
        let mut buffer = [0u8; RFC822_LEN];
        let len = self.format_rfc822(&mut buffer);
        String::from_utf8(buffer.get(..len).unwrap_or_default().to_vec()).unwrap_or_default()
    }

    /// Writes an RFC822 date without allocating.
    pub fn write_rfc822(&self, output: &mut impl Writer) {
        let mut buffer = [0u8; RFC822_LEN];
        let len = self.format_rfc822(&mut buffer);
        output.write(buffer.get(..len).unwrap_or_default());
    }
}

impl From<i64> for Date {
    fn from(datetime: i64) -> Self {
        Date::new(datetime)
    }
}

impl From<u64> for Date {
    fn from(datetime: u64) -> Self {
        Date::new(datetime as i64)
    }
}

impl Header for Date {
    fn write_header(&self, output: &mut impl Writer, _column: usize) {
        self.write_rfc822(output);
        output.write(b"\r\n");
    }
}
