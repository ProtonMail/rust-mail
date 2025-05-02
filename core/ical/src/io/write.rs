use super::*;
use std::fmt::{self, Write as _};

/// Object that can be serialized into an *.ics string; see [`Writer`].
pub trait Write<M> {
    fn write(&self, w: &mut Writer);

    #[allow(unused)]
    fn to_string(&self, _marker: M) -> String {
        let mut w = Writer::default();

        self.write(&mut w);

        w.finish().trim().to_owned()
    }
}

impl Write<Value> for bool {
    fn write(&self, w: &mut Writer) {
        w.raw(if *self { "TRUE" } else { "FALSE" });
    }
}

impl Write<Value> for i8 {
    fn write(&self, w: &mut Writer) {
        w.raw(format_args!("{self}"));
    }
}

impl Write<Value> for u8 {
    fn write(&self, w: &mut Writer) {
        w.raw(format_args!("{self}"));
    }
}

impl Write<Value> for i16 {
    fn write(&self, w: &mut Writer) {
        w.raw(format_args!("{self}"));
    }
}

impl Write<Value> for u16 {
    fn write(&self, w: &mut Writer) {
        w.raw(format_args!("{self}"));
    }
}

impl Write<Value> for i32 {
    fn write(&self, w: &mut Writer) {
        w.raw(format_args!("{self}"));
    }
}

impl Write<Value> for u32 {
    fn write(&self, w: &mut Writer) {
        w.raw(format_args!("{self}"));
    }
}

impl<M, T> Write<M> for &T
where
    T: Write<M> + ?Sized,
{
    fn write(&self, w: &mut Writer) {
        T::write(self, w);
    }
}

impl<T> Write<Value> for Vec<T>
where
    T: Write<Value>,
{
    fn write(&self, w: &mut Writer) {
        for (idx, value) in self.iter().enumerate() {
            if idx > 0 {
                w.raw(",");
            }

            w.value(value);
        }
    }
}

pub trait WriteRaw {
    fn write_raw(&self, w: &mut Writer);
}

impl WriteRaw for &str {
    fn write_raw(&self, w: &mut Writer) {
        _ = w.buffer.write_str(self);
    }
}

impl WriteRaw for fmt::Arguments<'_> {
    fn write_raw(&self, w: &mut Writer) {
        _ = w.buffer.write_fmt(*self);
    }
}
