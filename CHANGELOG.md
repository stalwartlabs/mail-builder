mail-builder 0.6.0
================================
- Breaking: Serialization writes into the new `Writer` sink (implemented for `Vec<u8>`, with `IoWriter` adapting any `std::io::Write`). `Header::write_header` now takes `&mut impl Writer` and a column, `MimePart::write_part` takes `&mut impl Writer`, and `generate_message_id_header` takes `&mut impl Writer` and returns nothing. `MessageBuilder::write_to`, `write_body`, `write_to_vec` and `write_to_string` are unchanged; `serialize` and `serialize_body` write into any `Writer`.
- Breaking: Requires Rust 1.88 or later.
- Added `base64_encode_slice` and `base64_encoded_len` (allocation-free base64 primitive, 2 to 3 times faster than the previous encoder), `Base64Encoder::encode_into`, `QuotedPrintableEncoder::encode_into`, `Date::write_rfc822` and `mime::write_boundary`.
- Performance: base64, quoted-printable, 7bit and header serialization rewritten to work on runs instead of bytes; no `format!` or per-byte writer calls remain on the serialization path; fixed per-message costs (hostname lookup, boundary generation, date formatting, output growth) removed.
- Changed: Header folding follows RFC 5322 and RFC 2047 (folds are CRLF followed by whitespace, no trailing whitespace before a fold, encoded-words at most 75 characters and never split inside a UTF-8 character); nested address lists are flattened instead of panicking; empty Message-ID and URL lists still terminate the header; quoted-printable lines are at most 76 characters including the soft line break; dates before 1970 are formatted correctly.
- Changed: Boundaries keep their shape but are generated from a per-thread counter instead of a hash of the thread id; the hostname used for generated Message-IDs is read once per process.
- Fix: Bare CR or LF in display names, group names, subjects, raw header values and parameter values can no longer reach the output.
- Fix: A raw multipart `Content-Type` header value that already contains `boundary="` is written instead of being dropped, and multipart parts with an unexpected `Content-Type` header value no longer panic.

mail-builder 0.5.0
================================
- Breaking: The `base64_encode`, `base64_encode_mime`, `get_encoding_type`, `rfc2047_encode`, `quoted_printable_encode`, `quoted_printable_encode_byte` and `inline_quoted_printable_encode` functions are no longer public. Use the new `Base64Encoder` and `QuotedPrintableEncoder` types instead (#50).
- Breaking: `EncodingType` is no longer part of the public API.
- Breaking: Updated to Rust edition 2024, which requires Rust 1.85 or later.
- Fix: Display names are no longer wrapped in a quoted-string when RFC 2047 encoded, and `Q`-encoded phrases now escape all characters outside the restricted set.

mail-builder 0.4.4
================================
- Do not split UTF-8 characters between encoded-words (#41)
- Escape underscores for quoted printable encoding (#39) (#43)
- Fix `Content-Transfer-Encoding` auto-detection for single long lines (#44) (#45)

mail-builder 0.4.3
================================
- Fix: Duplicate semicolon in group addresses.

mail-builder 0.4.2
================================
- Fix: Add semicolon at the end of group addresses.

mail-builder 0.4.1
================================
- Fix: Try to avoid lines longer than 78 characters (#32)

mail-builder 0.4.0
================================
- Removed `ludicrous` feature, the Rust compiler is smart enough to optimize array lookups.

mail-builder 0.3.2
================================
- Made `gethostname` crate optional.

mail-builder 0.3.1
================================
- Added `MimePart::transfer_encoding` method to disable automatic `Content-Transfer-Encoding` detection and treat it as a raw MIME part.

mail-builder 0.3.0
================================
- Replaced all `Multipart::new*` methods with a single `Multipart::new` method.

mail-builder 0.2.4
================================
- Added "Ludicrous mode" unsafe option for fast encoding.

mail-builder 0.2.3
================================
- Removed chrono dependency.

mail-builder 0.2.2
================================
- Fix: Generate valid Message-IDs

mail-builder 0.2.1
================================
- Fixed URL serializing bug.
- Headers are stored in a `Vec` instead of `BTreeMap`.

mail-builder 0.2.0
================================
- Improved API
- Added `write_to_vec` and `write_to_string`.

mail-builder 0.1.3
================================
- Bug fixes.
- Headers are written sorted alphabetically.
- Improved ID boundary generation.
- Encoding type detection for `[u8]` text parts.
- Optimised quoted-printable encoding.

mail-builder 0.1.2
================================
- All functions now take `impl Cow<str>`.

mail-builder 0.1.1
================================
- API improvements.

mail-builder 0.1.0
================================
- Initial release.
