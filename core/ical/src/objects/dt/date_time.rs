use super::*;

/// Date, time, and the associated form (describing the time zone etc.).
///
/// <https://www.rfc-editor.org/rfc/rfc5545.html#section-3.3.5>
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DateTime<F = AnyForm> {
    pub date: Date,
    pub time: Time,
    pub form: F,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DtValueType {
    Date,
    #[default]
    DateTime,
}

impl Read<Value> for DtValueType {
    fn read(r: &mut Reader) -> Option<Self> {
        let value = r.value::<Spanned<ParamValue>>()?;
        let (span, value) = (value.span, value.as_str());

        if value.eq_ignore_ascii_case("DATE") {
            Some(DtValueType::Date)
        } else if value.eq_ignore_ascii_case("DATE-TIME") {
            Some(DtValueType::DateTime)
        } else {
            r.error(span, format!("unknown value `{value}`"));
            None
        }
    }
}

impl Write<Value> for DtValueType {
    fn write(&self, w: &mut Writer) {
        w.raw(match self {
            DtValueType::Date => "DATE",
            DtValueType::DateTime => "DATE-TIME",
        });
    }
}
