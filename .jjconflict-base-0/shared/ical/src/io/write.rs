use super::*;
use std::fmt::{self, Write as _};

/// Object that can be serialized into an *.ics string; see [`IcsWriter`].
pub trait IcsWrite<M> {
    fn write(&self, w: &mut IcsWriter);

    fn to_string(&self, _marker: M) -> String {
        let mut w = IcsWriter::default();

        self.write(&mut w);

        w.finish().trim().to_owned()
    }
}

impl IcsWrite<Value> for bool {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(if *self { "TRUE" } else { "FALSE" });
    }
}

impl IcsWrite<Value> for i8 {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(format_args!("{self}"));
    }
}

impl IcsWrite<Value> for u8 {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(format_args!("{self}"));
    }
}

impl IcsWrite<Value> for i16 {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(format_args!("{self}"));
    }
}

impl IcsWrite<Value> for u16 {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(format_args!("{self}"));
    }
}

impl IcsWrite<Value> for i32 {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(format_args!("{self}"));
    }
}

impl IcsWrite<Value> for u32 {
    fn write(&self, w: &mut IcsWriter) {
        w.raw(format_args!("{self}"));
    }
}

impl<M, T> IcsWrite<M> for &T
where
    T: IcsWrite<M> + ?Sized,
{
    fn write(&self, w: &mut IcsWriter) {
        T::write(self, w);
    }
}

impl<T> IcsWrite<Value> for Vec<T>
where
    T: IcsWrite<Value>,
{
    fn write(&self, w: &mut IcsWriter) {
        for (idx, value) in self.iter().enumerate() {
            if idx > 0 {
                w.raw(",");
            }

            w.value(value);
        }
    }
}

pub trait WriteRaw {
    fn write_raw(&self, w: &mut IcsWriter);
}

impl WriteRaw for &str {
    fn write_raw(&self, w: &mut IcsWriter) {
        _ = w.buffer.write_str(self);
    }
}

impl WriteRaw for fmt::Arguments<'_> {
    fn write_raw(&self, w: &mut IcsWriter) {
        _ = w.buffer.write_fmt(*self);
    }
}
