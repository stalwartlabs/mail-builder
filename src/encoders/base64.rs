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

const CHARPAD: u8 = b'=';

#[inline(always)]
pub fn base64_encode(input: &[u8]) -> io::Result<Vec<u8>> {
    let mut buf = Vec::with_capacity(4 * (input.len() / 3));
    base64_encode_mime(input, &mut buf, true)?;
    Ok(buf)
}

pub fn base64_encode_mime(
    input: &[u8],
    mut output: impl Write,
    is_inline: bool,
) -> io::Result<usize> {
    let mut i = 0;
    let mut t1;
    let mut t2;
    let mut t3;
    let mut bytes_written = 0;

    if input.len() > 2 {
        while i < input.len() - 2 {
            t1 = input[i];
            t2 = input[i + 1];
            t3 = input[i + 2];

            output.write_all(&[
                E0[t1 as usize],
                E1[(((t1 & 0x03) << 4) | ((t2 >> 4) & 0x0F)) as usize],
                E1[(((t2 & 0x0F) << 2) | ((t3 >> 6) & 0x03)) as usize],
                E2[t3 as usize],
            ])?;

            bytes_written += 4;

            if !is_inline && bytes_written % 19 == 0 {
                output.write_all(b"\r\n")?;
            }

            i += 3;
        }
    }

    let remaining = input.len() - i;
    if remaining > 0 {
        t1 = input[i];
        if remaining == 1 {
            output.write_all(&[
                E0[t1 as usize],
                E1[((t1 & 0x03) << 4) as usize],
                CHARPAD,
                CHARPAD,
            ])?;
        } else {
            t2 = input[i + 1];
            output.write_all(&[
                E0[t1 as usize],
                E1[(((t1 & 0x03) << 4) | ((t2 >> 4) & 0x0F)) as usize],
                E2[((t2 & 0x0F) << 2) as usize],
                CHARPAD,
            ])?;
        }

        bytes_written += 4;

        if !is_inline && bytes_written % 19 == 0 {
            output.write_all(b"\r\n")?;
        }
    }

    if !is_inline && bytes_written % 19 != 0 {
        output.write_all(b"\r\n")?;
    }

    Ok(bytes_written)
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)]
mod tests {

    #[test]
    fn encode_base64() {
        for (input, expected_result, is_inline) in [
            ("Test".to_string(), "VGVzdA==\r\n", false),
            ("Ye".to_string(), "WWU=\r\n", false),
            ("A".to_string(), "QQ==\r\n", false),
            ("ro".to_string(), "cm8=\r\n", false),
            (
                "Are you a Shimano or Campagnolo person?".to_string(),
                "QXJlIHlvdSBhIFNoaW1hbm8gb3IgQ2FtcGFnbm9sbyBwZXJzb24/\r\n",
                false,
            ),
            (
                "<!DOCTYPE html>\n<html>\n<body>\n</body>\n</html>\n".to_string(),
                "PCFET0NUWVBFIGh0bWw+CjxodG1sPgo8Ym9keT4KPC9ib2R5Pgo8L2h0bWw+Cg==\r\n",
                false,
            ),
            ("áéíóú".to_string(), "w6HDqcOtw7PDug==\r\n", false),
            (
                " ".repeat(100),
                concat!(
                    "ICAgICAgICAgICAgICAgICAgICAgICAgICAgICAg",
                    "ICAgICAgICAgICAgICAgICAgICAgICAgICAg\r\n",
                    "ICAgICAgICAgICAgICAgICAgICAgICAgICAgICAg",
                    "ICAgICAgICAgICAgIA==\r\n",
                ),
                false,
            ),
        ] {
            let mut output = Vec::new();
            super::base64_encode_mime(input.as_bytes(), &mut output, is_inline).unwrap();
            assert_eq!(std::str::from_utf8(&output).unwrap(), expected_result);
        }
    }
}

/*
 * Table adapted from Nick Galbreath's "High performance base64 encoder / decoder"
 *
 * Copyright 2005, 2006, 2007 Nick Galbreath -- nickg [at] modp [dot] com
 * All rights reserved.
 *
 * http://code.google.com/p/stringencoders/
 *
 * Released under bsd license.
 *
 */

pub static E0: &[u8] = b"AAAABBBBCCCCDDDDEEEEFFFFGGGGHHHHIIIIJJJJKKKKLLLLMMMMNNNNOOOOPPPPQQQQRRRRSSSSTTTTUUUUVVVVWWWWXXXXYYYYZZZZaaaabbbbccccddddeeeeffffgggghhhhiiiijjjjkkkkllllmmmmnnnnooooppppqqqqrrrrssssttttuuuuvvvvwwwwxxxxyyyyzzzz0000111122223333444455556666777788889999++++////";
pub static E1: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
pub static E2: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
