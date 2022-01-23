use std::io::{self, Write};

use chrono::{LocalResult, TimeZone, Utc};

use super::Header;

pub struct Date {
    pub date: i64,
}

impl Date {
    pub fn new(date: i64) -> Self {
        Self { date }
    }

    pub fn now() -> Self {
        Self {
            date: Utc::now().timestamp(),
        }
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
