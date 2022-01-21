use std::borrow::Cow;

pub struct Date<'x> {
    pub date: Cow<'x, str>,
}
