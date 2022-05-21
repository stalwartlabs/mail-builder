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

use std::io::{self, Write};

use chrono::{DateTime, LocalResult, TimeZone, Utc};

use super::Header;

/// RFC5322 Date header
pub struct Date {
    pub date: i64,
}

impl Date {
    /// Create a new Date header from a `chrono` timestamp.
    pub fn new(date: i64) -> Self {
        Self { date }
    }

    /// Create a new Date header using the current time.
    pub fn now() -> Self {
        Self {
            date: Utc::now().timestamp(),
        }
    }
}

impl From<DateTime<Utc>> for Date {
    fn from(datetime: DateTime<Utc>) -> Self {
        Date::new(datetime.timestamp())
    }
}

impl From<&DateTime<Utc>> for Date {
    fn from(datetime: &DateTime<Utc>) -> Self {
        Date::new(datetime.timestamp())
    }
}

impl Header for Date {
    fn write_header(&self, mut output: impl Write, _bytes_written: usize) -> io::Result<usize> {
        if let LocalResult::Single(dt) = Utc.timestamp_opt(self.date, 0) {
            output.write_all(dt.to_rfc2822().as_bytes())?;
        }
        output.write_all(b"\r\n")?;
        Ok(0)
    }
}
