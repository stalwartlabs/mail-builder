use std::borrow::Cow;

use crate::encoders::encode::rfc2047_encode;

use super::Header;

pub struct EmailAddress<'x> {
    pub name: Option<Cow<'x, str>>,
    pub email: Cow<'x, str>,
}

pub struct GroupedAddresses<'x> {
    pub name: Option<Cow<'x, str>>,
    pub addresses: Vec<EmailAddress<'x>>,
}

pub enum Address<'x> {
    Address(EmailAddress<'x>),
    Group(GroupedAddresses<'x>),
    List(Vec<Address<'x>>),
}

impl<'x> Header for Address<'x> {
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
                                output.write_all(b"; ")?;
                                bytes_written += 1;
                            }
                        }
                        Address::List(_) => unreachable!(),
                    }

                    if bytes_written > 76 && pos < list.len() - 1 {
                        output.write_all(b"\r\n\t")?;
                        bytes_written = 0;
                    }
                }
            }
        }
        output.write_all(b"\r\n")?;
        Ok(0)
    }
}

impl<'x> Header for EmailAddress<'x> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        _line_len: usize,
    ) -> std::io::Result<usize> {
        let mut bytes_written = 0;
        if let Some(name) = &self.name {
            bytes_written += rfc2047_encode(name.as_ref(), &mut output)? + 1;
            output.write_all(b" ")?;
        }

        output.write_all(b"<")?;
        output.write_all(self.email.as_bytes())?;
        output.write_all(b">")?;

        Ok(bytes_written + self.email.len() + 2)
    }
}

impl<'x> Header for GroupedAddresses<'x> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        _line_len: usize,
    ) -> std::io::Result<usize> {
        let mut bytes_written = 0;
        if let Some(name) = &self.name {
            bytes_written += rfc2047_encode(name.as_ref(), &mut output)? + 2;
            output.write_all(b": ")?;
        }

        for (pos, address) in self.addresses.iter().enumerate() {
            if pos > 0 {
                output.write_all(b", ")?;
                bytes_written += 2;
            }

            bytes_written += address.write_header(&mut output, 0)?;

            if bytes_written > 76 && pos < self.addresses.len() - 1 {
                output.write_all(b"\r\n\t")?;
                bytes_written = 0;
            }
        }

        Ok(bytes_written)
    }
}
