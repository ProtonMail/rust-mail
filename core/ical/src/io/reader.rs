use super::*;
use std::fmt::Write as _;
use std::{fmt, mem};

/// *.ics-aware reader.
///
/// This is akin to [`std::str::FromStr`], but specialized for the *.ics format.
#[derive(Debug)]
pub struct IcsReader<'a> {
    src: &'a [u8],
    pos: usize,
    msgs: Vec<ReadMsg>,
    context: Vec<Spanned<String>>,
}

impl<'a> IcsReader<'a> {
    #[must_use]
    pub fn new(src: &'a [u8]) -> Self {
        Self {
            src,
            pos: 0,
            msgs: Vec::new(),
            context: Vec::new(),
        }
    }

    #[must_use]
    pub fn pos(&self) -> usize {
        self.pos
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.src.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pos() >= self.len()
    }

    #[must_use]
    pub fn finish(self) -> Vec<ReadMsg> {
        self.msgs
    }

    fn msg<M>(&mut self, at: impl Into<Option<Span>>, msg: M, kind: ReadMsgKind)
    where
        M: fmt::Display,
    {
        self.msgs.push(ReadMsg {
            at: at.into(),
            msg: msg.to_string(),
            kind,
            context: self.context.clone(),
        });
    }

    /// Reports a warning.
    pub fn warn<M>(&mut self, at: impl Into<Option<Span>>, msg: M)
    where
        M: fmt::Display,
    {
        self.msg(at, msg, ReadMsgKind::Warning);
    }

    /// Reports an error.
    pub fn error<M>(&mut self, at: impl Into<Option<Span>>, msg: M)
    where
        M: fmt::Display,
    {
        self.msg(at, msg, ReadMsgKind::Error);
    }

    /// Runs `f` under a parser that contains an extra context such as "parsing
    /// `foo`".
    pub fn context<T>(&mut self, span: Span, tag: String, f: impl FnOnce(&mut Self) -> T) -> T {
        self.context.push(Spanned::new(span, tag));
        let result = f(self);
        self.context.pop();

        result
    }

    /// Runs `f` and either commits or discards its parse result, depending on
    /// whether it returns `Some` or `None`; useful for lookahead.
    #[must_use]
    pub fn attempt<T>(&mut self, f: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let mut this = IcsReader {
            src: self.src,
            pos: self.pos,
            msgs: Vec::new(),
            context: Vec::new(),
        };

        if let Some(val) = f(&mut this) {
            self.pos = this.pos;

            self.msgs.extend(this.msgs.into_iter().map(|mut entry| {
                entry.context = self.context.iter().cloned().chain(entry.context).collect();
                entry
            }));

            self.context.extend(this.context);

            Some(val)
        } else {
            None
        }
    }

    /// Runs `f` ignoring all messages it raises; useful for recovery.
    pub fn silently<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let msgs = mem::take(&mut self.msgs);
        let value = f(self);

        self.msgs = msgs;

        value
    }

    /// Runs `f` and returns the scanned object together with its encompassing
    /// span.
    #[must_use]
    pub fn spanned<T>(&mut self, f: impl FnOnce(&mut Self) -> Option<T>) -> Option<Spanned<T>> {
        let pos = self.pos;
        let value = f(self)?;

        Some(Spanned::new(Span::new(pos, self.pos), value))
    }

    /// Returns the next byte, part of [`Self::char()`].
    ///
    /// Note that we perform line-unfolding right here to properly handle *.ics
    /// which have `\r\n ` inserted over code points, as the RFC points out:
    ///
    /// > Note: It is possible for very simple implementations to generate
    /// > improperly folded lines in the middle of a UTF-8 multi-octet
    /// > sequence.  For this reason, implementations need to unfold lines
    /// > in such a way to properly restore the original sequence.
    fn byte(&mut self) -> Option<u8> {
        loop {
            let c0 = self.src.get(self.pos);
            let c1 = self.src.get(self.pos + 1);

            match (c0, c1) {
                (Some(b'\r'), _) => {
                    self.pos += 1;
                }
                (Some(b'\n'), Some(b' ' | b'\t')) => {
                    self.pos += 2;
                }
                (Some(ch), _) => {
                    self.pos += 1;
                    break Some(*ch);
                }
                (None, _) => {
                    break None;
                }
            }
        }
    }

    /// Returns the next character.
    ///
    /// This function performs line-unfolding, so given:
    ///
    /// ```text
    /// FOO:This is a lo
    ///  ng foo.
    /// BAR:This is bar.
    /// ```
    ///
    /// ... the "partial newline" between `lo` and `ng` will be skipped over and
    /// this function will return `\n` only after `foo.` and `bar.`.
    #[must_use]
    pub fn char(&mut self) -> Option<char> {
        loop {
            let pos = self.pos;
            let mut buf = [0, 0, 0, 0];

            buf[0] = self.byte()?;

            for idx in 1..utf8_width::get_width(buf[0]) {
                buf[idx] = self.byte()?;
            }

            if let Some(ch) = buf.utf8_chunks().next() {
                if let Some(ch) = ch.valid().chars().next() {
                    break Some(ch);
                }
            }

            self.warn(Span::new(pos, pos + 1), "invalid unicode character");
        }
    }

    /// Returns the next character without consuming it.
    ///
    /// See: [`Self::char()`].
    #[must_use]
    pub fn peek(&mut self) -> Option<char> {
        let mut ch = None;

        _ = self.attempt::<()>(|this| {
            ch = this.char();
            None
        });

        ch
    }

    /// Eats the next character if it matches `ch`.
    ///
    /// See: [`Self::char()`].
    #[must_use]
    pub fn try_eat(&mut self, ch: char) -> Option<()> {
        self.attempt(|this| {
            let got = this.char()?;

            if got.eq_ignore_ascii_case(&ch) {
                Some(())
            } else {
                None
            }
        })
    }

    /// Eats the next character if it matches `ch`, throws an error otherwise.
    ///
    /// See: [`Self::peek()`].
    #[must_use]
    pub fn eat(&mut self, ch: char) -> Option<()> {
        if self.try_eat(ch).is_some() {
            Some(())
        } else {
            let msg = self.peek().map_or_else(
                || "incomplete source".into(),
                |got| format!("expected {ch:?}, got {got:?}"),
            );

            self.error(Span::new(self.pos, self.pos + 1), msg);

            None
        }
    }

    /// Eats the next characters.
    pub fn try_string(&mut self, s: &str) -> Option<()> {
        self.attempt(|this| {
            for ch in s.chars() {
                this.eat(ch)?;
            }

            Some(())
        })
    }

    /// Eats the next character if it's a digit.
    ///
    /// See: [`Self::digit()`].
    #[must_use]
    pub fn try_digit(&mut self) -> Option<char> {
        self.attempt(|this| {
            let got = this.char()?;

            if got.is_ascii_digit() {
                Some(got)
            } else {
                None
            }
        })
    }

    /// Eats the next character if it's a digit (0-9) and returns it; throws an
    /// error if there's no digit ahead of us.
    ///
    /// See: [`Self::try_digit()`].
    #[must_use]
    pub fn digit(&mut self) -> Option<char> {
        if let Some(digit) = self.try_digit() {
            Some(digit)
        } else {
            let msg = self.peek().map_or_else(
                || "incomplete source".into(),
                |got| format!("expected digit (0-9), got {got:?}"),
            );

            self.error(Span::new(self.pos, self.pos + 1), msg);

            None
        }
    }

    /// Eats a number that consists of exactly `digits` digits (0-9) and returns
    /// it; throws an error if there's no such number ahead of us.
    #[must_use]
    pub fn digits(&mut self, digits: usize) -> Option<u32> {
        self.spanned(|this| {
            let mut num = String::new();

            for _ in 0..digits {
                num.push(this.digit()?);
            }

            Some(num.parse())
        })?
        .unwrap(self)
    }

    /// Eats an identifier (e.g. `BEGIN`, `ORGANIZER`, `X-SOMETHING` etc.) and
    /// returns it; throws an error if there's no identifier ahead of us.
    #[must_use]
    pub fn ident(&mut self) -> Option<String> {
        let mut value = String::new();

        let Some(got) = self.peek() else {
            self.error(Span::new(self.pos, self.pos + 1), "incomplete source");
            return None;
        };

        if !got.is_alphabetic() {
            self.error(
                Span::new(self.pos, self.pos + 1),
                format!("expected identifier (a-zA-Z), got {got:?}"),
            );

            return None;
        }

        _ = self.char();
        value.push(got);

        while let Some(ch) = self.peek() {
            if ch.is_alphabetic() || ch == '-' {
                _ = self.char();
                value.push(ch);
            } else {
                break;
            }
        }

        Some(value)
    }

    /// Eats the rest of the line and returns it.
    ///
    /// See [`Self::char()`] for the definition of line.
    #[must_use]
    pub fn rest(&mut self) -> String {
        let mut value = String::new();

        while let Some(ch) = self.char() {
            if ch == '\n' {
                break;
            }

            value.push(ch);
        }

        value
    }

    /// Infers what kind of thing is in front of us (a component, a property,
    /// etc.), eats it, and returns it.
    #[must_use]
    pub fn entry(&mut self) -> Option<ReadEntry> {
        if self.try_eat(':').is_some() {
            return Some(ReadEntry::Value);
        }

        if self.try_eat(';').is_some() {
            let name = self.spanned(Self::ident)?;

            self.eat('=')?;

            return Some(ReadEntry::Param { name });
        }

        let name = loop {
            if let Some(name) = self.spanned(Self::ident) {
                break name;
            }

            // Recover by skipping rest of the line
            self.silently(IcsReader::rest);

            if self.is_empty() {
                return None;
            }
        };

        if name.eq_ignore_ascii_case("BEGIN") {
            self.eat(':')?;

            Some(ReadEntry::Comp {
                name: self.spanned(|this| Some(this.rest()))?,
            })
        } else if name.eq_ignore_ascii_case("END") {
            self.eat(':')?;

            Some(ReadEntry::CompEnd {
                name: self.spanned(|this| Some(this.rest()))?,
            })
        } else {
            Some(ReadEntry::Prop { name })
        }
    }

    #[must_use]
    pub fn unwrap_prop<T>(&mut self, name: &str, value: Option<T>) -> Option<T>
    where
        T: IcsRead<Property>,
    {
        value.or_else(|| {
            let default = T::reasonable_default();

            if default.is_some() {
                self.warn(None, format!("missing property `{name}`"));
            } else {
                self.error(None, format!("missing property `{name}`"));
            }

            default
        })
    }

    /// Eats all the parameters that follow and throws the "unknown parameter"
    /// message for each.
    pub fn burn_params(&mut self) -> Option<()> {
        loop {
            let entry = self.attempt(|this| {
                if let ReadEntry::Param { name } = this.entry()? {
                    Some(ReadEntry::Param { name })
                } else {
                    None
                }
            });

            if let Some(entry) = entry {
                entry.burn(self);
            } else {
                break;
            }
        }

        // Parameters are supposed to be followed by value, as in:
        //
        // ```
        // FOO=TRUE;BAR=FALSE:something
        // ```
        //
        // ... so let's go ahead and consume the value's marker as well.
        self.eat(':')
    }

    #[must_use]
    pub fn prop<T>(&mut self) -> Option<T>
    where
        T: IcsRead<Property>,
    {
        self.any()
    }

    #[must_use]
    pub fn value<T>(&mut self) -> Option<T>
    where
        T: IcsRead<Value>,
    {
        self.any()
    }

    #[must_use]
    fn any<T, M>(&mut self) -> Option<T>
    where
        T: IcsRead<M>,
    {
        self.context(
            Span::new(self.pos, self.pos + 1),
            format!("`{}`", T::name()),
            T::read,
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadMsg {
    pub at: Option<Span>,
    pub msg: String,
    pub kind: ReadMsgKind,
    pub context: Vec<Spanned<String>>,
}

impl ReadMsg {
    /// Converts this message into string.
    ///
    /// If you provide `src`, i.e. the buffer passed to `VCalendar::from_str()`
    /// etc., the returned message will contain more information about the
    /// context in which the error occurred.
    #[must_use]
    pub fn to_string<'a>(&self, src: impl Into<Option<&'a [u8]>>) -> String {
        let mut out = String::new();

        _ = write!(
            out,
            "{}: ",
            match self.kind {
                ReadMsgKind::Warning => "warn",
                ReadMsgKind::Error => "error",
            }
        );

        _ = write!(out, "{}", self.msg);

        if let Some(src) = src.into() {
            _ = writeln!(out);

            if let Some(at) = self.at {
                _ = writeln!(out, " --> at {}", at.resolve(src));
            }

            for ctx in self.context.iter().rev() {
                _ = writeln!(
                    out,
                    "  | when parsing {} at {}",
                    ctx.as_str(),
                    ctx.span.resolve(src)
                );
            }
        }

        out
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadMsgKind {
    /// A warning - the calendar has been parsed correctly, but some parts of it
    /// don't strictly conform to the RFC.
    Warning,

    /// An error - the calendar has been parsed *partially*, some parts of it
    /// might be missing.
    Error,
}

#[derive(Clone, Debug)]
pub enum ReadEntry {
    /// Component's beginning, as in `BEGIN:NAME`.
    Comp { name: Spanned<String> },

    /// Component ending, as in `END:NAME`.
    CompEnd { name: Spanned<String> },

    /// Property, as in `NAME=`.
    Prop { name: Spanned<String> },

    /// Parameter, as in `;NAME`.
    Param { name: Spanned<String> },

    /// Parameter's value, as in `:something`.
    Value,
}

impl ReadEntry {
    /// If this entry begins a component named `name`, reads that component and
    /// returns true; returns false otherwise.
    #[must_use]
    pub fn try_comp<T>(&self, r: &mut IcsReader, name: &str, value: &mut Option<T>) -> bool
    where
        T: IcsRead<Component>,
    {
        let ReadEntry::Comp { name: this } = self else {
            return false;
        };

        if !this.eq_ignore_ascii_case(name) {
            return false;
        }

        if value.is_some() {
            r.error(this.span, format!("duplicated component `{name}`"));
        }

        *value = r.any();

        true
    }

    /// Like [`Self::try_comp()`], but allows to read multiple components.
    #[must_use]
    pub fn try_comps<T>(&self, r: &mut IcsReader, name: &str, values: &mut Vec<T>) -> bool
    where
        T: IcsRead<Component>,
    {
        let mut value = None;

        if self.try_comp(r, name, &mut value) {
            if let Some(value) = value {
                values.push(value);
            }

            true
        } else {
            false
        }
    }

    /// If this entry ends a component named `name`, returns true; returns false
    /// otherwise.
    #[must_use]
    pub fn try_comp_end(&self, name: &str) -> bool {
        let ReadEntry::CompEnd { name: this } = self else {
            return false;
        };

        this.eq_ignore_ascii_case(name)
    }

    /// If this entry begins a property named `name`, reads that property and
    /// returns true; returns false otherwise.
    #[must_use]
    pub fn try_prop<T>(&self, r: &mut IcsReader, name: &str, value: &mut Option<T>) -> bool
    where
        T: IcsRead<Property>,
    {
        let ReadEntry::Prop { name: this } = self else {
            return false;
        };

        if !this.eq_ignore_ascii_case(name) {
            return false;
        }

        if value.is_some() {
            r.error(this.span, format!("duplicated property `{name}`"));
        }

        *value = if let Some(value) = r.any() {
            // `T` should've eaten the newline character, but let's eat it
            // here as well just in case
            _ = r.try_eat('\n');

            Some(value)
        } else {
            // Recover by skipping rest of the line
            r.silently(IcsReader::rest);

            T::reasonable_default()
        };

        true
    }

    /// Like [`Self::try_prop()`], but allows to read multiple properties.
    #[must_use]
    pub fn try_props<T>(&self, r: &mut IcsReader, name: &str, values: &mut Vec<T>) -> bool
    where
        T: IcsRead<Property>,
    {
        let mut value = None;

        if self.try_prop(r, name, &mut value) {
            if let Some(value) = value {
                values.push(value);
            }

            true
        } else {
            false
        }
    }

    /// If this entry begins a parameter named `name`, reads that parameter and
    /// returs true; returns false otherwise.
    #[must_use]
    pub fn try_param<T>(&self, r: &mut IcsReader, name: &str, value: &mut Option<T>) -> bool
    where
        T: IcsRead<Value>,
    {
        let ReadEntry::Param { name: this } = self else {
            return false;
        };

        if !this.eq_ignore_ascii_case(name) {
            return false;
        }

        if value.is_some() {
            r.error(this.span, format!("duplicated parameter `{name}`"));
        }

        *value = r.any().or_else(T::reasonable_default);

        true
    }

    /// Returns whether this entry is a value.
    #[must_use]
    pub fn is_value(&self) -> bool {
        matches!(self, ReadEntry::Value)
    }

    /// Throws the "unknown property / component / ..." error and recovers.
    pub fn burn(self, r: &mut IcsReader) {
        match self {
            ReadEntry::Comp { name } => {
                r.error(name.span, format!("unknown component `{}`", name.value));

                // Recover by skipping to the matching `END:` line
                r.silently(|r| {
                    r.attempt(|r| {
                        while let Some(e) = r.entry() {
                            if e.try_comp_end(name.as_str()) {
                                return Some(());
                            }

                            e.burn(r);
                        }

                        None::<()>
                    })
                });
            }

            ReadEntry::CompEnd { name } => {
                r.error(name.span, "mismatched `END:`");

                // No need to recover, the entire `END:SOMETHING` part has been
                // already read
            }

            ReadEntry::Prop { name } => {
                // `X-` prefix (as in `X-FOO`) marks a vendor property - we
                // don't have to support those and they shouldn't affect the
                // semantics of the calendar in a major way, so let's emit those
                // as warnings instead of errors
                let msg = format!("unknown property `{}`", name.value);

                if name.starts_with("x-") || name.starts_with("X-") {
                    r.warn(name.span, msg);
                } else {
                    r.error(name.span, msg);
                }

                // Recover by skipping rest of the line
                r.silently(IcsReader::rest);
            }

            ReadEntry::Param { name } => {
                // Same as with properties, the `X-` prefix marks something
                // vendor-specific - if we happen not to support anything, no
                // need to sweat over it
                let msg = format!("unknown parameter `{}`", name.value);

                if name.starts_with("x-") || name.starts_with("X-") {
                    r.warn(name.span, msg);
                } else {
                    r.error(name.span, msg);
                }

                // Recover by skipping the parameter
                r.silently(|r| {
                    while let Some(ch) = r.peek() {
                        if ch == ':' || ch == ';' {
                            break;
                        }

                        _ = r.char();
                    }
                });
            }

            ReadEntry::Value => {
                r.error(Span::new(r.pos() - 1, r.pos()), "unexpected value");

                // Recover by skipping rest of the line
                r.silently(IcsReader::rest);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ics;
    use pretty_assertions as pa;
    use test_case::test_case;

    fn target(s: impl Into<String>) -> IcsReader<'static> {
        IcsReader::new(Box::new(s.into()).leak().as_bytes())
    }

    #[test]
    fn folding() {
        let mut r = target(ics! {"
            hel
             lo
        "});

        assert_eq!("hello", r.rest());

        // ---

        let mut r = target(ics! {"
            hel
            \tlo
        "});

        assert_eq!("hello", r.rest());
    }

    #[test_case("BEGIN:VCALENDAR", Some("BEGIN"))]
    #[test_case("VERSION:2.0", Some("VERSION"))]
    #[test_case("X-SOMETHING:123", Some("X-SOMETHING"))]
    #[test_case("-", None ; "minus")]
    #[test_case(";", None ; "semicolon")]
    fn ident(s: &str, expected: Option<&str>) {
        assert_eq!(expected, target(s).ident().as_deref());
    }

    #[test]
    fn err_unknown_component() {
        let mut r = target(ics! {"
            BEGIN:VEVENT
            BEGIN:VALARM
            END:VALARM
            END:VEVENT
        "});

        r.entry().unwrap().burn(&mut r);

        assert!(r.entry().is_none());

        let actual = r.finish();

        let expected = vec![
            ReadMsg {
                at: Some(Span::new(6, 14)),
                msg: "unknown component `VEVENT`".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            },
            ReadMsg {
                at: Some(Span::new(50, 51)),
                msg: "incomplete source".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            },
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn err_unknown_property() {
        let mut r = target(ics! {"
            FOO:one
            BAR:two-three/four
            ZAR:five
            X-TEST:six
        "});

        r.entry().unwrap().burn(&mut r);
        r.entry().unwrap().burn(&mut r);
        r.entry().unwrap().burn(&mut r);
        r.entry().unwrap().burn(&mut r);

        assert!(r.entry().is_none());

        let actual = r.finish();

        let expected = vec![
            ReadMsg {
                at: Some(Span::new(0, 3)),
                msg: "unknown property `FOO`".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            },
            ReadMsg {
                at: Some(Span::new(9, 12)),
                msg: "unknown property `BAR`".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            },
            ReadMsg {
                at: Some(Span::new(29, 32)),
                msg: "unknown property `ZAR`".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            },
            ReadMsg {
                at: Some(Span::new(39, 45)),
                msg: "unknown property `X-TEST`".into(),
                kind: ReadMsgKind::Warning,
                context: Vec::new(),
            },
            ReadMsg {
                at: Some(Span::new(49, 50)),
                msg: "incomplete source".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            },
        ];

        pa::assert_eq!(expected, actual);
    }

    #[test]
    fn err_unknown_parameter() {
        let mut r = target(ics! {"
            ;FOO=one;BAR=two-tree/four;ZAR=five
        "});

        r.entry().unwrap().burn(&mut r);
        r.entry().unwrap().burn(&mut r);
        r.entry().unwrap().burn(&mut r);

        assert!(r.entry().is_none());

        let actual = r.finish();

        let expected = vec![
            ReadMsg {
                at: Some(Span::new(1, 4)),
                msg: "unknown parameter `FOO`".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            },
            ReadMsg {
                at: Some(Span::new(9, 12)),
                msg: "unknown parameter `BAR`".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            },
            ReadMsg {
                at: Some(Span::new(27, 30)),
                msg: "unknown parameter `ZAR`".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            },
            ReadMsg {
                at: Some(Span::new(35, 36)),
                msg: "incomplete source".into(),
                kind: ReadMsgKind::Error,
                context: Vec::new(),
            },
        ];

        pa::assert_eq!(expected, actual);
    }
}
