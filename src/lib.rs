pub mod encoders;
pub mod headers;
pub mod mime;

use std::{
    collections::HashMap,
    io::{self, Write},
};

use chrono::Local;
use headers::{
    address::Address, date::Date, message_id::MessageId, text::Text, Header, HeaderType,
};
use mime::{make_boundary, MimePart};

pub struct MessageBuilder<'x> {
    pub headers: HashMap<String, Vec<HeaderType<'x>>>,
    pub html_body: Option<MimePart<'x>>,
    pub text_body: Option<MimePart<'x>>,
    pub attachments: Option<Vec<MimePart<'x>>>,
    pub body: Option<MimePart<'x>>,
}

impl<'x> Default for MessageBuilder<'x> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'x> MessageBuilder<'x> {
    pub fn new() -> Self {
        MessageBuilder {
            headers: HashMap::new(),
            html_body: None,
            text_body: None,
            attachments: None,
            body: None,
        }
    }

    pub fn message_id(&mut self, value: MessageId<'x>) {
        self.header("Message-ID", value.into());
    }

    pub fn in_reply_to(&mut self, value: MessageId<'x>) {
        self.header("In-Reply-To", value.into());
    }

    pub fn references(&mut self, value: MessageId<'x>) {
        self.header("References", value.into());
    }

    pub fn sender(&mut self, value: Address<'x>) {
        self.header("Sender", value.into());
    }

    pub fn from(&mut self, value: Address<'x>) {
        self.header("From", value.into());
    }

    pub fn to(&mut self, value: Address<'x>) {
        self.header("To", value.into());
    }

    pub fn cc(&mut self, value: Address<'x>) {
        self.header("Cc", value.into());
    }

    pub fn bcc(&mut self, value: Address<'x>) {
        self.header("Bcc", value.into());
    }

    pub fn reply_to(&mut self, value: Address<'x>) {
        self.header("Reply-To", value.into());
    }

    pub fn subject(&mut self, value: &'x str) {
        self.header("From", Text::new(value).into());
    }

    pub fn date(&mut self, value: Date) {
        self.header("Date", value.into());
    }

    pub fn header(&mut self, header: &str, value: HeaderType<'x>) {
        self.headers
            .entry(header.to_string())
            .or_insert_with(Vec::new)
            .push(value);
    }

    pub fn text_body(&mut self, value: &'x str) {
        self.text_body = Some(MimePart::new_text(value));
    }

    pub fn html_body(&mut self, value: &'x str) {
        self.html_body = Some(MimePart::new_html(value));
    }

    pub fn attachment(&mut self, content_type: &'x str, filename: &'x str, value: &'x [u8]) {
        self.attachments
            .get_or_insert_with(Vec::new)
            .push(MimePart::new_binary(content_type, value).attachment(filename));
    }

    pub fn inline_binary(&mut self, content_type: &'x str, cid: &'x str, value: &'x [u8]) {
        self.attachments
            .get_or_insert_with(Vec::new)
            .push(MimePart::new_binary(content_type, value).inline().cid(cid));
    }

    pub fn body(&mut self, value: MimePart<'x>) {
        self.body = Some(value);
    }

    pub fn write_to(self, mut output: impl Write) -> io::Result<()> {
        let mut has_date = false;
        let mut has_message_id = false;

        for (header_name, header_values) in &self.headers {
            if !has_date && header_name == "Date" {
                has_date = true;
            } else if !has_message_id && header_name == "Message-ID" {
                has_message_id = true;
            }

            for header_value in header_values {
                output.write_all(header_name.as_bytes())?;
                output.write_all(b": ")?;
                header_value.write_header(&mut output, header_name.len() + 2)?;
            }
        }

        if !has_message_id {
            output.write_all(b"Message-ID: <")?;
            output.write_all(make_boundary().as_bytes())?;
            output.write_all(b">\r\n")?;
        }

        if !has_date {
            output.write_all(b"Date: ")?;
            output.write_all(Local::now().to_rfc2822().as_bytes())?;
            output.write_all(b"\r\n")?;
        }

        (if let Some(body) = self.body {
            body
        } else {
            match (self.text_body, self.html_body, self.attachments) {
                (Some(text), Some(html), Some(attachments)) => {
                    let mut parts = Vec::with_capacity(attachments.len() + 1);
                    parts.push(MimePart::new_multipart(
                        "multipart/alternative",
                        vec![text, html],
                    ));
                    parts.extend(attachments);

                    MimePart::new_multipart("multipart/mixed", parts)
                }
                (Some(text), Some(html), None) => {
                    MimePart::new_multipart("multipart/alternative", vec![text, html])
                }
                (Some(text), None, Some(attachments)) => {
                    let mut parts = Vec::with_capacity(attachments.len() + 1);
                    parts.push(text);
                    parts.extend(attachments);
                    MimePart::new_multipart("multipart/mixed", parts)
                }
                (Some(text), None, None) => text,
                (None, Some(html), Some(attachments)) => {
                    let mut parts = Vec::with_capacity(attachments.len() + 1);
                    parts.push(html);
                    parts.extend(attachments);
                    MimePart::new_multipart("multipart/mixed", parts)
                }
                (None, Some(html), None) => html,
                (None, None, Some(attachments)) => {
                    MimePart::new_multipart("multipart/mixed", attachments)
                }
                (None, None, None) => MimePart::new_text("\n"),
            }
        })
        .write_part(output)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs::File;

    use crate::{
        headers::{address::Address, url::URL},
        MessageBuilder,
    };

    #[test]
    fn build_message() {
        let mut builder = MessageBuilder::new();
        builder.from(Address::new_address("John Doe".into(), "john@doe.com"));
        builder.to(Address::List(vec![
            Address::new_address("Antoine de Saint-Exupéry".into(), "antoine@exupery.com"),
            Address::new_address("안녕하세요 세계".into(), "test@test.com"),
            Address::new_address("Xin chào".into(), "addr@addr.com"),
        ]));
        builder.bcc(Address::List(vec![
            Address::new_group(
                "Привет, мир".into(),
                vec![
                    Address::new_address("My ascii name".into(), "addr1@addr7.com"),
                    Address::new_address("ハロー・ワールド".into(), "addr2@addr6.com"),
                    Address::new_address("áéíóú".into(), "addr3@addr5.com"),
                    Address::new_address("Γειά σου Κόσμε".into(), "addr4@addr4.com"),
                ],
            ),
            Address::new_group(
                "Hello world".into(),
                vec![
                    Address::new_address("שלום עולם".into(), "addr5@addr3.com"),
                    Address::new_address("¡El ñandú comió ñoquis!".into(), "addr6@addr2.com"),
                    Address::new_address(None, "addr7@addr1.com"),
                ],
            ),
        ]));
        builder.header(
            "List-Archive",
            URL::new("http://example.com/archive").into(),
        );

        let text_body = "Hello, world!\n".repeat(20);
        let html_body = "<p>¡Hola Mundo!</p>".repeat(20);
        let attachments = vec!["안녕하세요 세계".repeat(20), "ハロー・ワールド".repeat(20)];

        builder.text_body(&text_body);
        builder.html_body(&html_body);
        builder.inline_binary("image/png", "cid:image", &[0, 1, 2, 3, 4, 5]);
        builder.attachment("text/plain", "my fílé.txt", attachments[0].as_bytes());
        builder.attachment("text/plain", "ハロー・ワールド", attachments[1].as_bytes());

        builder.write_to(File::create("test.eml").unwrap()).unwrap();
    }
}
