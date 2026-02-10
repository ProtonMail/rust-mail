use super::*;
use std::fmt;

/// *.ics-aware writer.
///
/// This is akin to [`std::fmt::Formatter`], but specialized for the *.ics
/// format.
#[derive(Debug, Default)]
pub struct IcsWriter {
    pub(crate) buffer: IcsWriterBuffer,
}

impl IcsWriter {
    /// Writes a component (e.g. `VEVENT`).
    pub fn comp(&mut self, name: &str, value: impl IcsWrite<Component>) {
        debug_assert!(
            Self::is_name_valid(name),
            "component name is invalid: `{name}`"
        );
        debug_assert!(
            self.buffer.line_len == 0,
            "component must be written into an empty line"
        );

        self.raw(format_args!("BEGIN:{name}\n"));
        value.write(self);
        self.raw(format_args!("END:{name}\n"));
    }

    /// Writes a property (`NAME:VALUE`).
    #[allow(clippy::needless_pass_by_value)]
    pub fn prop(&mut self, name: &str, value: impl IcsWrite<Property>) {
        debug_assert!(
            Self::is_name_valid(name),
            "property name is invalid: `{name}`"
        );
        debug_assert!(
            self.buffer.line_len == 0,
            "property must be written into an empty line"
        );

        self.raw(name);
        value.write(self);
        self.raw("\n");
    }

    /// Writes a property, if it's present.
    pub fn prop_opt(&mut self, name: &str, value: Option<impl IcsWrite<Property>>) {
        if let Some(prop) = value {
            self.prop(name, prop);
        }
    }

    /// Writes a parameter (`;NAME=VALUE`).
    pub fn param(&mut self, name: &str, value: impl IcsWrite<Value>) {
        debug_assert!(
            Self::is_name_valid(name),
            "parameter name is invalid: `{name}`"
        );

        // Separate consecutive parameters with a semicolon.
        //
        // It's a bit awkward, because parameters can appear in two positions -
        // as a part of the name or the value, e.g. given:
        //
        // ```
        // DTSTART;TZID=America/New_York:19970902T090000
        // RRULE:FREQ=DAILY;UNTIL=19971224T000000Z
        // ```
        //
        // - the `TZID` parameter is part of the name, so it needs to be
        //   separated,
        //
        // - the `FREQ` parameter appears as the *first* value-parameter, so it
        //   doesn't need to be separated (that's this `:` check we do here),
        //
        // - the `UNTIL` parameter is the *second* value-parameter, so it needs
        //   to be separated.
        //
        // As an alternative, we could add a `needs_semicolon: bool` param to
        // this function, but that'd require for the programmer to remember the
        // context in which given parameter appears, which can be error-prone.
        let out = &self.buffer.out;

        if out.is_empty()
            || (out.is_char_boundary(out.len() - 1) && out.as_bytes()[out.len() - 1] != b':')
        {
            self.raw(";");
        }

        self.raw(format_args!("{name}="));
        value.write(self);
    }

    /// Writes a parameter, if it's present.
    pub fn param_opt(&mut self, name: &str, value: Option<impl IcsWrite<Value>>) {
        if let Some(value) = value {
            self.param(name, value);
        }
    }

    /// Writes a value, e.g. a [`Text`].
    pub fn value(&mut self, value: impl IcsWrite<Value>) {
        value.write(self);
    }

    /// Writes a value verbatim, without escaping newlines etc.
    ///
    /// To keep the basic requirements in check, `\n` still gets converted into
    /// `\r\n` and line-wrapping is still applied.
    ///
    /// This function should be used carefully - usually you'll want to go
    /// through [`Self::value()`].
    #[allow(clippy::needless_pass_by_value)]
    pub fn raw(&mut self, value: impl WriteRaw) {
        value.write_raw(self);
    }

    /// Completes the writing process and returns an *.ics string representing
    /// the serialized structure.
    ///
    /// String returned by this function is already line-wrapped according to
    /// the specification, no extra post-processing needs to be done.
    #[must_use]
    pub fn finish(self) -> String {
        self.buffer.out
    }

    fn is_name_valid(name: &str) -> bool {
        !name.is_empty() && name.chars().all(|ch| ch.is_ascii_uppercase() || ch == '-')
    }
}

#[derive(Debug, Default)]
pub(crate) struct IcsWriterBuffer {
    out: String,
    line_len: usize,
}

/// Note that writing into this buffer doesn't escape newlines - in fact, it
/// cannot, since at this point we don't know the context in which given string
/// appears so it might be totally fine it contains actual, god-loving newlines.
///
/// To prevent people from shooting themselves in feet, this implementation is
/// hidden within the private [`IcsWriterBuffer`] and exposed to users only via
/// [`IcsWriter::raw()`] which explicitly mentions this caveat.
impl fmt::Write for IcsWriterBuffer {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        // Opportunistically wrap lines that are too long, as per
        // https://www.rfc-editor.org/rfc/rfc5545.html#section-3.1
        //
        // Note that splitting on code points seems to be sooorta legal:
        //
        // > Note: It is possible for very simple implementations to generate
        // > improperly folded lines in the middle of a UTF-8 multi-octet
        // > sequence.  For this reason, implementations need to unfold lines
        // > in such a way to properly restore the original sequence.
        //
        // ... but we're being nice and avoid doing so - we might still split
        // the string on graphemes, but that's alright, no need to get fancy.

        for ch in s.chars() {
            if ch == '\n' {
                self.out.push_str("\r\n");
                self.line_len = 0;
            } else {
                if self.line_len + ch.len_utf8() > 75 {
                    self.out.push_str("\r\n ");
                    self.line_len = 1;
                }

                self.out.push(ch);
                self.line_len += ch.len_utf8();
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Text;
    use itertools::Itertools;

    #[test]
    fn folding_ascii() {
        let mut target = IcsWriter::default();

        target.raw(format_args!(
            "DESCRIPTION: It is impossible to live in the past, difficult to \
             live in the present and a waste to live in the future."
        ));

        let expected = [
            "DESCRIPTION: It is impossible to live in the past, difficult to live in the",
            "  present and a waste to live in the future.",
        ]
        .iter()
        .join("\r\n");

        assert_eq!(expected, target.finish());
    }

    #[test]
    fn folding_unicode() {
        let mut target = IcsWriter::default();

        target.raw(format_args!(
            "foo 💘 bar 🔧 foo 🔳 bar 😶 foo 🚯 bar 🎇 foo 🛏 bar 🎙 foo 🐡 \
             bar 🗨 foo 📍 bar🖇 foo 🌾 bar 💈 foo 🙇 bar 🔹 foo 🏸 bar 📉"
        ));

        let expected = [
            "foo 💘 bar 🔧 foo 🔳 bar 😶 foo 🚯 bar 🎇 foo 🛏 bar 🎙 foo",
            "  🐡 bar 🗨 foo 📍 bar🖇 foo 🌾 bar 💈 foo 🙇 bar 🔹 foo ",
            " 🏸 bar 📉",
        ]
        .iter()
        .join("\r\n");

        assert_eq!(expected, target.finish());
    }

    #[test]
    fn strings() {
        let mut target = IcsWriter::default();

        target.param(
            "LYRICS",
            Text::new("Where have all the good men gone\nAnd where are all the gods?"),
        );

        assert_eq!(
            ";LYRICS=Where have all the good men gone\\nAnd where are all the gods?",
            target.finish()
        );
    }
}
