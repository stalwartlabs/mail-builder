/*
 * SPDX-FileCopyrightText: 2020 Stalwart Labs LLC <hello@stalw.art>
 *
 * SPDX-License-Identifier: Apache-2.0 OR MIT
 */

use super::{
    Header,
    fold::{FOLD_TARGET, FoldWriter},
    rfc2047::write_phrase,
};
use crate::writer::Writer;
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

const TAIL_NONE: &[u8] = b"";
const TAIL_COMMA: &[u8] = b",";

impl Header for Address<'_> {
    fn write_header(&self, output: &mut impl Writer, column: usize) {
        if let Address::Address(address) = self
            && address.name.is_none()
            && column + address.email.len() + 2 <= FOLD_TARGET
        {
            output.write_byte(b'<');
            output.write(address.email.as_bytes());
            output.write(b">\r\n");
            return;
        }

        let mut folder = FoldWriter::new(output, column);
        self.write_value(&mut folder, TAIL_NONE);
        folder.finish();
    }
}

impl Address<'_> {
    #[inline(always)]
    fn writes_nothing(&self, in_group: bool) -> bool {
        match self {
            Address::Address(_) => false,
            Address::Group(group) => {
                let named = group.name.is_some();
                (in_group || !named)
                    && group
                        .addresses
                        .iter()
                        .all(|address| address.writes_nothing(in_group || named))
            }
            Address::List(list) => list.iter().all(|address| address.writes_nothing(in_group)),
        }
    }

    fn write_value<W: Writer>(&self, folder: &mut FoldWriter<'_, W>, tail: &[u8]) {
        match self {
            Address::Address(address) => address.write_mailbox(folder, tail),
            Address::Group(group) => group.write_group(folder, tail),
            Address::List(list) => write_list(folder, list, tail, false),
        }
    }
}

fn write_list<W: Writer>(
    folder: &mut FoldWriter<'_, W>,
    list: &[Address<'_>],
    tail: &[u8],
    in_group: bool,
) {
    let Some(last) = list
        .iter()
        .rposition(|address| !address.writes_nothing(in_group))
    else {
        return;
    };

    for (pos, address) in list.iter().enumerate() {
        if pos > last {
            return;
        }
        if address.writes_nothing(in_group) {
            continue;
        }

        let item_tail = if pos == last { tail } else { TAIL_COMMA };

        match address {
            Address::Address(address) => address.write_mailbox(folder, item_tail),
            Address::Group(group) if in_group => {
                write_list(folder, &group.addresses, item_tail, true)
            }
            Address::Group(group) => group.write_group(folder, item_tail),
            Address::List(nested) => write_list(folder, nested, item_tail, in_group),
        }

        if pos == last {
            return;
        }
        folder.space();
    }
}

impl EmailAddress<'_> {
    #[inline]
    pub(crate) fn write_mailbox<W: Writer>(&self, folder: &mut FoldWriter<'_, W>, tail: &[u8]) {
        if let Some(name) = &self.name {
            write_phrase(folder, name, TAIL_NONE);
            folder.space();
        }

        folder.begin_atom(self.email.len() + 2 + tail.len());
        folder.write_byte(b'<');
        folder.write(self.email.as_bytes());
        match tail {
            [] => folder.write_byte(b'>'),
            [b','] => folder.write(b">,"),
            [b';'] => folder.write(b">;"),
            _ => {
                folder.write_byte(b'>');
                folder.write_tail(tail);
            }
        }
    }
}

impl Header for EmailAddress<'_> {
    fn write_header(&self, output: &mut impl Writer, column: usize) {
        let mut folder = FoldWriter::new(output, column);
        self.write_mailbox(&mut folder, TAIL_NONE);
        folder.finish();
    }
}

impl GroupedAddresses<'_> {
    pub(crate) fn write_group<W: Writer>(&self, folder: &mut FoldWriter<'_, W>, tail: &[u8]) {
        let Some(name) = &self.name else {
            return write_list(folder, &self.addresses, tail, false);
        };

        let is_empty = self
            .addresses
            .iter()
            .all(|address| address.writes_nothing(true));
        let (name_tail, list_tail): (&[u8], &[u8]) = if tail.is_empty() {
            (b":;", b";")
        } else {
            (b":;,", b";,")
        };
        let name_tail = if is_empty { name_tail } else { b":" };

        write_phrase(folder, name, name_tail);

        if !is_empty {
            folder.space();
            write_list(folder, &self.addresses, list_tail, true);
        }
    }
}

impl Header for GroupedAddresses<'_> {
    fn write_header(&self, output: &mut impl Writer, column: usize) {
        let mut folder = FoldWriter::new(output, column);
        self.write_group(&mut folder, TAIL_NONE);
        folder.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mail_parser::MessageParser;

    #[test]
    fn nameless_groups_are_written_as_their_mailboxes() {
        let header = build(Address::new_group(
            None::<&str>,
            vec![
                Address::new_address(None::<&str>, "a@x.test"),
                Address::new_address(None::<&str>, "b@x.test"),
            ],
        ));
        assert_eq!(header, "<a@x.test>, <b@x.test>\r\n");

        let header = build(Address::new_list(vec![
            Address::new_group(
                Some("List 1"),
                vec![Address::new_address(None::<&str>, "a@x.test")],
            ),
            Address::new_group(
                None::<&str>,
                vec![
                    Address::new_address(None::<&str>, "b@x.test"),
                    Address::new_address(None::<&str>, "c@x.test"),
                ],
            ),
        ]));
        assert_eq!(
            header,
            "\"List 1\": <a@x.test>;, <b@x.test>, <c@x.test>\r\n"
        );
        assert_eq!(
            parse(&header),
            vec![
                (None, Some("a@x.test".to_string())),
                (None, Some("b@x.test".to_string())),
                (None, Some("c@x.test".to_string())),
            ]
        );

        let header = build(Address::new_group(None::<&str>, vec![]));
        assert_eq!(header, "\r\n");

        let header = build(Address::new_list(vec![
            Address::new_group(None::<&str>, vec![]),
            Address::new_address(None::<&str>, "a@x.test"),
            Address::new_group(None::<&str>, vec![]),
        ]));
        assert_eq!(header, "<a@x.test>\r\n");
    }

    #[test]
    fn group_with_empty_nested_group_last_is_terminated() {
        let header = build(Address::new_group(
            Some("Team"),
            vec![
                Address::new_address(None::<&str>, "a@b.com"),
                Address::new_group(Some("Inner"), vec![]),
            ],
        ));
        assert_eq!(header, "\"Team\": <a@b.com>;\r\n");
        let header = build(Address::new_group(
            Some("Team"),
            vec![Address::new_list(vec![Address::new_group(
                Some("Inner"),
                vec![],
            )])],
        ));
        assert_eq!(header, "\"Team\":;\r\n");
    }

    fn build(address: Address<'_>) -> String {
        let mut output = Vec::new();
        address.write_header(&mut output, 4);
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
