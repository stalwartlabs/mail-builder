use crate::encoders::encode::rfc2047_encode;

use super::Header;

pub struct EmailAddress<'x> {
    pub name: Option<&'x str>,
    pub email: &'x str,
}

pub struct GroupedAddresses<'x> {
    pub name: Option<&'x str>,
    pub addresses: Vec<Address<'x>>,
}

pub enum Address<'x> {
    Address(EmailAddress<'x>),
    Group(GroupedAddresses<'x>),
    List(Vec<Address<'x>>),
}

impl<'x> Address<'x> {
    pub fn new_address(name: Option<&'x str>, email: &'x str) -> Self {
        Address::Address(EmailAddress { name, email })
    }
    pub fn new_group(name: Option<&'x str>, addresses: Vec<Address<'x>>) -> Self {
        Address::Group(GroupedAddresses { name, addresses })
    }
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
                    if bytes_written
                        + (match address {
                            Address::Address(address) => {
                                address.email.len() + address.name.map_or(0, |n| n.len() + 3) + 2
                            }
                            Address::Group(group) => group.name.map_or(0, |name| name.len() + 2),
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
                                output.write_all(b"; ")?;
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

impl<'x> Header for EmailAddress<'x> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        mut bytes_written: usize,
    ) -> std::io::Result<usize> {
        if let Some(name) = &self.name {
            println!("{} <{}>: {}", name, self.email, bytes_written);
            bytes_written += rfc2047_encode(name, &mut output)?;
            println!(
                "{} <{}>: {}",
                name,
                self.email,
                bytes_written + self.email.len() + 2
            );
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

impl<'x> Header for GroupedAddresses<'x> {
    fn write_header(
        &self,
        mut output: impl std::io::Write,
        mut bytes_written: usize,
    ) -> std::io::Result<usize> {
        if let Some(name) = &self.name {
            bytes_written += rfc2047_encode(name, &mut output)? + 2;
            output.write_all(b": ")?;
        }

        for (pos, address) in self.addresses.iter().enumerate() {
            let address = address.unwrap_address();

            if bytes_written + address.email.len() + address.name.map_or(0, |n| n.len() + 3) + 2
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

        Ok(bytes_written)
    }
}
