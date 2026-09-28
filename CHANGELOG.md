# Change Log

All notable changes to this project will be documented in this file. This project adheres to [Semantic Versioning](https://semver.org/).

## [2.0.0] - 2026-09-28

### Changed
- Base64 and quoted-printable encoding is now done by the `encodify` crate.

### Removed
- **Breaking:** The `encoders` module is no longer public. `Base64Encoder`, `QuotedPrintableEncoder`, `base64_encode_slice`, `base64_encoded_len` and the `E0`, `E1` and `E2` tables are no longer exported; callers should use `encodify` instead.

## [1.0.0] - 2026-09-12

### Added
- `base64_encode_slice` and `base64_encoded_len`, an allocation-free base64 primitive 2 to 3 times faster than the previous encoder.
- `Base64Encoder::encode_into` and `QuotedPrintableEncoder::encode_into`.
- `Date::write_rfc822` and `mime::write_boundary`.

### Changed
- **Breaking:** Serialization writes into the new `Writer` sink (implemented for `Vec<u8>`, with `IoWriter` adapting any `std::io::Write`). `Header::write_header` now takes `&mut impl Writer` and a column, `MimePart::write_part` takes `&mut impl Writer`, and `generate_message_id_header` takes `&mut impl Writer` and returns nothing. `MessageBuilder::write_to`, `write_body`, `write_to_vec` and `write_to_string` are unchanged; `serialize` and `serialize_body` write into any `Writer`.
- **Breaking:** Requires Rust 1.88 or later.
- Base64, quoted-printable, 7bit and header serialization rewritten to work on runs instead of bytes; no `format!` or per-byte writer calls remain on the serialization path; fixed per-message costs (hostname lookup, boundary generation, date formatting, output growth) removed.
- Header folding follows RFC 5322 and RFC 2047 (folds are CRLF followed by whitespace, no trailing whitespace before a fold, encoded-words at most 75 characters and never split inside a UTF-8 character).
- Boundaries keep their shape but are generated from a per-thread counter instead of a hash of the thread id; the hostname used for generated Message-IDs is read once per process.
- `Content-Type` and `Content-Disposition` parameter values that contain non-ASCII or control characters are written as RFC 2231 extended parameters (`filename*=UTF-8''...`, split into `*0*`, `*1*` sections when long) instead of RFC 2047 encoded-words inside a quoted string, which RFC 2047 forbids.

### Fixed
- Bare CR or LF in display names, group names, subjects, raw header values and parameter values can no longer reach the output.
- A raw multipart `Content-Type` header value that already contains `boundary="` is written instead of being dropped, and multipart parts with an unexpected `Content-Type` header value no longer panic.

## [0.5.0] - 2026-08-18

### Changed
- **Breaking:** The `base64_encode`, `base64_encode_mime`, `get_encoding_type`, `rfc2047_encode`, `quoted_printable_encode`, `quoted_printable_encode_byte` and `inline_quoted_printable_encode` functions are no longer public. Use the new `Base64Encoder` and `QuotedPrintableEncoder` types instead (#50).
- **Breaking:** `EncodingType` is no longer part of the public API.
- **Breaking:** Updated to Rust edition 2024, which requires Rust 1.85 or later.

### Fixed
- Display names are no longer wrapped in a quoted-string when RFC 2047 encoded, and `Q`-encoded phrases now escape all characters outside the restricted set.

## [0.4.4] - 2025-08-12

### Fixed
- Do not split UTF-8 characters between encoded-words (#41).
- Escape underscores for quoted printable encoding (#39, #43).
- `Content-Transfer-Encoding` auto-detection for single long lines (#44, #45).

## [0.4.3] - 2025-05-06

### Fixed
- Duplicate semicolon in group addresses.

## [0.4.2] - 2025-03-09

### Fixed
- Add semicolon at the end of group addresses.

## [0.4.1] - 2025-02-13

### Fixed
- Avoid lines longer than 78 characters (#32).

## [0.4.0] - 2025-01-26

### Removed
- `ludicrous` feature; the Rust compiler is smart enough to optimize array lookups.

## [0.3.2] - 2024-07-10

### Changed
- Made the `gethostname` crate optional.

## [0.3.1] - 2023-10-02

### Added
- `MimePart::transfer_encoding` method to disable automatic `Content-Transfer-Encoding` detection and treat it as a raw MIME part.

## [0.3.0] - 2023-06-02

### Changed
- **Breaking:** Replaced all `Multipart::new*` methods with a single `Multipart::new` method.

## [0.2.5] - 2023-01-19

### Added
- `MessageBuilder`, `MimePart` and `BodyPart` now derive `Clone`.

### Changed
- **Breaking:** `generate_message_id_header` now takes a `hostname` parameter instead of reading it internally via `gethostname`.

### Fixed
- Quoted-printable encoding of header values no longer applies body-only folding rules (a bare `\n` in a header value was written as `\r\n`, corrupting the header).

## [0.2.4] - 2022-11-03

### Added
- "Ludicrous mode" unsafe option for fast encoding.

## [0.2.3] - 2022-10-21

### Removed
- `chrono` dependency.

## [0.2.2] - 2022-08-31

### Fixed
- Generate valid Message-IDs.

## [0.2.1] - 2022-08-02

### Changed
- Headers are stored in a `Vec` instead of `BTreeMap`.

### Fixed
- URL serializing bug.

## [0.2.0] - 2022-05-20

### Added
- `write_to_vec` and `write_to_string`.

### Changed
- Improved API.

## [0.1.3] - 2022-02-16

### Added
- Encoding type detection for `[u8]` text parts.

### Changed
- Headers are written sorted alphabetically.
- Improved ID boundary generation.
- Optimised quoted-printable encoding.

### Fixed
- Bug fixes.

## [0.1.2] - 2022-01-26

### Changed
- All functions now take `impl Cow<str>`.

## [0.1.1] - 2022-01-25

### Changed
- API improvements.

## [0.1.0] - 2022-01-24

### Added
- Initial release.
