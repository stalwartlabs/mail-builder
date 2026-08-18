/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use super::Header;
use crate::encoders::encode::rfc2047_encode_phrase;
use std::borrow::Cow;

/// RFC5322 e-mail address
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct EmailAddress<'x> {
    pub name: Option<Cow<'x, str>>,
    pub email: Cow<'x, str>,
}

/// RFC5322 grouped e-mail addresses
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct GroupedAddresses<'x> {
    pub name: Option<Cow<'x, str>>,
    pub addresses: Vec<Address<'x>>,
}

/// RFC5322 address
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Address<'x> {
    Address(EmailAddress<'x>),
    Group(GroupedAddresses<'x>),
    List(Vec<Address<'x>>),
}

impl<'x> Address<'x> {
    /// Create an RFC5322 e-mail address
    pub fn new_address(
        name: Option<impl Into<Cow<'x, str>>>,
        email: impl Into<Cow<'x, str>>,
    ) -> Self {
        Address::Address(EmailAddress {
            name: name.map(|v| v.into()),
            email: email.into(),
        })
    }

    /// Create an RFC5322 grouped e-mail address
    pub fn new_group(name: Option<impl Into<Cow<'x, str>>>, addresses: Vec<Address<'x>>) -> Self {
        Address::Group(GroupedAddresses {
            name: name.map(|v| v.into()),
            addresses,
        })
    }

    /// Create an address list
    pub fn new_list(items: Vec<Address<'x>>) -> Self {
        Address::List(items)
    }

    pub fn unwrap_address(&self) -> &EmailAddress<'x> {
        match self {
            Address::Address(address) => address,
            _ => panic!("Address is not an EmailAddress"),
        }
    }
}

impl<'x> From<(&'x str, &'x str)> for Address<'x> {
    fn from(value: (&'x str, &'x str)) -> Self {
        Address::Address(EmailAddress {
            name: Some(value.0.into()),
            email: value.1.into(),
        })
    }
}

impl From<(String, String)> for Address<'_> {
    fn from(value: (String, String)) -> Self {
        Address::Address(EmailAddress {
            name: Some(value.0.into()),
            email: value.1.into(),
        })
    }
}

impl<'x> From<&'x str> for Address<'x> {
    fn from(value: &'x str) -> Self {
        Address::Address(EmailAddress {
            name: None,
            email: value.into(),
        })
    }
}

impl From<String> for Address<'_> {
    fn from(value: String) -> Self {
        Address::Address(EmailAddress {
            name: None,
            email: value.into(),
        })
    }
}

impl<'x, T> From<Vec<T>> for Address<'x>
where
    T: Into<Address<'x>>,
{
    fn from(value: Vec<T>) -> Self {
        Address::new_list(value.into_iter().map(|x| x.into()).collect())
    }
}

impl<'x, T, U> From<(U, Vec<T>)> for Address<'x>
where
    T: Into<Address<'x>>,
    U: Into<Cow<'x, str>>,
{
    fn from(value: (U, Vec<T>)) -> Self {
        Address::Group(GroupedAddresses {
            name: Some(value.0.into()),
            addresses: value.1.into_iter().map(|x| x.into()).collect(),
        })
    }
}

impl Header for Address<'_> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        mut bytes_written: usize,
    ) -> std::io::Result<usize> {
        match self {
            Address::Address(address) => {
                address.write_header(&mut output, bytes_written)?;
            }
            Address::Group(group) => {
                group.write_header(&mut output, bytes_written)?;
            }
            Address::List(list) => {
                for (pos, address) in list.iter().enumerate() {
                    if bytes_written
                        + (match address {
                            Address::Address(address) => {
                                address.email.len()
                                    + address.name.as_ref().map_or(0, |n| n.len() + 3)
                                    + 2
                            }
                            Address::Group(group) => {
                                group.name.as_ref().map_or(0, |name| name.len() + 2)
                            }
                            Address::List(_) => 0,
                        })
                        >= 76
                    {
                        output.write_all(b"\r\n\t")?;
                        bytes_written = 1;
                    }

                    match address {
                        Address::Address(address) => {
                            bytes_written += address.write_header(&mut output, bytes_written)?;
                            if pos < list.len() - 1 {
                                output.write_all(b", ")?;
                                bytes_written += 1;
                            }
                        }
                        Address::Group(group) => {
                            bytes_written += group.write_header(&mut output, bytes_written)?;
                            if pos < list.len() - 1 {
                                output.write_all(b" ")?;
                                bytes_written += 1;
                            }
                        }
                        Address::List(_) => unreachable!(),
                    }
                }
            }
        }
        output.write_all(b"\r\n")?;
        Ok(0)
    }
}

impl Header for EmailAddress<'_> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        mut bytes_written: usize,
    ) -> std::io::Result<usize> {
        if let Some(name) = &self.name {
            bytes_written += rfc2047_encode_phrase(name, &mut output)?;
            if bytes_written + self.email.len() + 2 >= 76 {
                output.write_all(b"\r\n\t")?;
                bytes_written = 1;
            } else {
                output.write_all(b" ")?;
                bytes_written += 1;
            }
        }

        output.write_all(b"<")?;
        output.write_all(self.email.as_bytes())?;
        output.write_all(b">")?;

        Ok(bytes_written + self.email.len() + 2)
    }
}

impl Header for GroupedAddresses<'_> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        mut bytes_written: usize,
    ) -> std::io::Result<usize> {
        if let Some(name) = &self.name {
            bytes_written += rfc2047_encode_phrase(name, &mut output)? + 2;
            output.write_all(b": ")?;
        }

        for (pos, address) in self.addresses.iter().enumerate() {
            let address = address.unwrap_address();

            if bytes_written
                + address.email.len()
                + address.name.as_ref().map_or(0, |n| n.len() + 3)
                + 2
                >= 76
            {
                output.write_all(b"\r\n\t")?;
                bytes_written = 1;
            }

            bytes_written += address.write_header(&mut output, bytes_written)?;
            if pos < self.addresses.len() - 1 {
                output.write_all(b", ")?;
                bytes_written += 2;
            }
        }

        output.write_all(b";")?;
        bytes_written += 1;

        Ok(bytes_written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mail_parser::MessageParser;

    fn build(address: Address<'_>) -> String {
        let mut output = Vec::new();
        address.write_header(&mut output, 4).unwrap();
        String::from_utf8(output).unwrap()
    }

    fn parse(header: &str) -> Vec<(Option<String>, Option<String>)> {
        let raw = format!("Cc: {header}\r\n");
        let message = MessageParser::new().parse_headers(raw.as_bytes()).unwrap();
        message
            .cc()
            .unwrap()
            .iter()
            .map(|addr| {
                (
                    addr.name().map(str::to_string),
                    addr.address().map(str::to_string),
                )
            })
            .collect()
    }

    #[test]
    fn encoded_display_name_is_not_wrapped_in_quotes() {
        let header = build(Address::new_address(
            Some("Anna Müller"),
            "anna@example.com",
        ));
        assert!(
            !header.contains("\"=?"),
            "quoted encoded-word in {header:?}"
        );
        assert!(header.contains("=?utf-8?"), "not encoded in {header:?}");
        assert_eq!(
            parse(&header),
            vec![(
                Some("Anna Müller".to_string()),
                Some("anna@example.com".to_string())
            )]
        );
    }

    #[test]
    fn base64_display_name_is_not_wrapped_in_quotes() {
        let name = "Δοκιμή, Εταιρεία";
        let header = build(Address::new_address(Some(name), "info@example.org"));
        assert!(header.contains("=?utf-8?B?"), "not base64 in {header:?}");
        assert!(
            !header.contains("\"=?"),
            "quoted encoded-word in {header:?}"
        );
        assert_eq!(
            parse(&header),
            vec![(Some(name.to_string()), Some("info@example.org".to_string()))]
        );
    }

    #[test]
    fn display_name_containing_quotes_and_comma_round_trips() {
        let name = "\"Steuerberater, Wirtschaftsprüfer\"";
        let header = build(Address::new_list(vec![
            Address::new_address(Some("Anna Müller"), "anna@example.com"),
            Address::new_address(Some(name), "kanzlei@example.org"),
        ]));

        assert!(
            !header.contains("\"=?"),
            "quoted encoded-word in {header:?}"
        );

        assert_eq!(
            parse(&header),
            vec![
                (
                    Some("Anna Müller".to_string()),
                    Some("anna@example.com".to_string())
                ),
                (
                    Some(name.to_string()),
                    Some("kanzlei@example.org".to_string())
                ),
            ],
            "{header:?}"
        );
    }

    #[test]
    fn comma_in_encoded_display_name_does_not_split_list() {
        let header = build(Address::new_list(vec![
            Address::new_address(Some("Müller, Anna"), "anna@example.com"),
            Address::new_address(Some("Beispiel GmbH"), "info@example.org"),
        ]));
        assert_eq!(
            header.matches(',').count(),
            1,
            "comma left unescaped inside encoded-word in {header:?}"
        );

        let parsed = parse(&header);
        assert_eq!(parsed.len(), 2, "{header:?}");
        assert_eq!(parsed[0].0.as_deref(), Some("Müller, Anna"), "{header:?}");
    }

    #[test]
    fn phrase_encoded_word_escapes_specials() {
        let header = build(Address::new_address(
            Some("Meier (Kanzlei) <x>; [y] \"z\" @ w\\v: u, tü"),
            "info@example.org",
        ));
        assert!(header.contains("?Q?"), "not Q encoded in {header:?}");

        let encoded = header
            .split_once("?Q?")
            .unwrap()
            .1
            .split_once("?=")
            .unwrap()
            .0;

        for ch in [
            '(', ')', '<', '>', '[', ']', ':', ';', '@', '\\', ',', '"', '.',
        ] {
            assert!(!encoded.contains(ch), "raw {ch:?} in {encoded:?}");
        }

        assert_eq!(
            parse(&header),
            vec![(
                Some("Meier (Kanzlei) <x>; [y] \"z\" @ w\\v: u, tü".to_string()),
                Some("info@example.org".to_string())
            )]
        );
    }

    #[test]
    fn ascii_display_name_with_comma_uses_quoted_string() {
        let header = build(Address::new_address(Some("Doe, John"), "john@example.com"));
        assert!(header.contains("\"Doe, John\""), "{header:?}");
        assert_eq!(
            parse(&header),
            vec![(
                Some("Doe, John".to_string()),
                Some("john@example.com".to_string())
            )]
        );
    }

    #[test]
    fn ascii_display_name_with_quotes_is_escaped() {
        let name = "John \"JD\" Doe";
        let header = build(Address::new_address(Some(name), "john@example.com"));
        assert_eq!(
            parse(&header),
            vec![(Some(name.to_string()), Some("john@example.com".to_string()))],
            "{header:?}"
        );
    }

    #[test]
    fn group_name_with_specials_round_trips() {
        let header = build(Address::new_group(
            Some("Büro, Empfang"),
            vec![Address::new_address(
                Some("Anna Müller"),
                "anna@example.com",
            )],
        ));
        assert!(
            !header.contains("\"=?"),
            "quoted encoded-word in {header:?}"
        );

        let raw = format!("Cc: {header}\r\n");
        let message = MessageParser::new().parse_headers(raw.as_bytes()).unwrap();
        let groups = message.cc().unwrap().as_group().expect("not a group");

        assert_eq!(groups.len(), 1, "{header:?}");
        assert_eq!(
            groups[0].name.as_deref(),
            Some("Büro, Empfang"),
            "{header:?}"
        );
        assert_eq!(groups[0].addresses.len(), 1, "{header:?}");
        assert_eq!(
            groups[0].addresses[0].name.as_deref(),
            Some("Anna Müller"),
            "{header:?}"
        );
    }
}
