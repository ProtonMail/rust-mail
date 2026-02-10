use super::*;

/// Exception date-times.
///
/// <https://www.rfc-editor.org/rfc/rfc5545#section-3.8.5.1>
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExDate {
    Dates(Vec<Date>),
    DateTimes(AnyForm, Vec<(Date, Time)>),
}

impl IcsRead<Property> for ExDate {
    fn read(r: &mut IcsReader) -> Option<Self> {
        let mut value = None;
        let mut tzid = None;

        loop {
            let e = r.entry()?;

            if e.try_param(r, "VALUE", &mut value) || e.try_param(r, "TZID", &mut tzid) {
                continue;
            }

            if e.is_value() {
                break;
            }

            e.burn(r, Kind::Property)?;
        }

        match value.unwrap_or_default() {
            DtValueType::Date => Some(ExDate::Dates(r.value()?)),

            DtValueType::DateTime => {
                let dates: Vec<DateTime<_>> = r.value()?;

                let form = if let Some(tzid) = tzid {
                    AnyForm::Tz(tzid)
                } else if let Some(date) = dates.first() {
                    // N.B. we could validate that all dates have the same form,
                    //      as mixing local times with UTC times is forbidden,
                    //      but let's not go crazy
                    match date.form {
                        UtcOrLocalForm::Local => AnyForm::Local,
                        UtcOrLocalForm::Utc => AnyForm::Utc,
                    }
                } else {
                    return None;
                };

                let dates = dates.into_iter().map(|dt| (dt.date, dt.time)).collect();

                Some(ExDate::DateTimes(form, dates))
            }
        }
    }
}

impl IcsWrite<Property> for ExDate {
    fn write(&self, w: &mut IcsWriter) {
        match self {
            ExDate::Dates(_) => {
                w.param("VALUE", DtValueType::Date);
            }

            ExDate::DateTimes(form, _) => {
                // Implied `VALUE=DATE-TIME`

                if let AnyForm::Tz(tzid) = form {
                    w.param("TZID", tzid);
                }
            }
        }

        w.raw(":");

        match self {
            ExDate::Dates(days) => {
                w.value(days);
            }

            ExDate::DateTimes(form, days) => {
                for (idx, (date, time)) in days.iter().enumerate() {
                    if idx > 0 {
                        w.raw(",");
                    }

                    w.value(date);
                    w.raw("T");
                    w.value(time);

                    if let AnyForm::Utc = form {
                        w.raw("Z");
                    }
                }
            }
        }
    }
}

#[cfg(feature = "php")]
mod php {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq, ZvalConvert)]
    struct PhpExDate {
        kind: String,
        dates: Vec<DateTime<AnyForm>>,
    }

    impl From<ExDate> for PhpExDate {
        fn from(value: ExDate) -> Self {
            match value {
                ExDate::Dates(dates) => PhpExDate {
                    kind: "Dates".into(),

                    dates: dates
                        .iter()
                        .copied()
                        .map(|date| DateTime {
                            date,
                            time: Time::new_unchecked(0, 0, 0),
                            form: AnyForm::Utc,
                        })
                        .collect(),
                },

                ExDate::DateTimes(form, dates) => PhpExDate {
                    kind: "DateTimes".into(),

                    dates: dates
                        .iter()
                        .copied()
                        .map(|(date, time)| DateTime {
                            date,
                            time,
                            form: form.clone(),
                        })
                        .collect(),
                },
            }
        }
    }

    impl TryFrom<PhpExDate> for ExDate {
        type Error = ();

        fn try_from(value: PhpExDate) -> Result<Self, Self::Error> {
            match value.kind.as_str() {
                "Dates" => Ok(ExDate::Dates(
                    value.dates.into_iter().map(|dt| dt.date).collect(),
                )),

                "DateTimes" => {
                    let form = value
                        .dates
                        .first()
                        .map_or(AnyForm::Utc, |dt| dt.form.clone());

                    Ok(ExDate::DateTimes(
                        form,
                        value
                            .dates
                            .into_iter()
                            .map(|dt| (dt.date, dt.time))
                            .collect(),
                    ))
                }

                _ => Err(()),
            }
        }
    }

    impl<'a> FromPhpZval<'a> for ExDate {
        const TYPE: PhpDataType = PhpDataType::Object(None);

        fn from_zval(zval: &'a PhpZval) -> Option<Self> {
            PhpExDate::from_zval(zval)?.try_into().ok()
        }
    }

    impl IntoPhpZval for ExDate {
        const TYPE: PhpDataType = PhpDataType::Object(None);
        const NULLABLE: bool = false;

        fn set_zval(self, zval: &mut PhpZval, persistent: bool) -> PhpResult<()> {
            PhpExDate::from(self).set_zval(zval, persistent)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TzId;
    use crate::ics;
    use crate::utils::*;

    #[test]
    fn dates() {
        let target = ExDate::Dates(vec![
            d("20180101"),
            d("20180102"),
            d("20180103"),
            d("20180105"),
            d("20180110"),
        ]);

        let expected = ics! {"
            ;VALUE=DATE:20180101,20180102,20180103,20180105,20180110
        "};

        assert_eq!(expected, target.to_string(Property));
        assert_trip!(expected, ExDate as Property);
    }

    #[test]
    fn local_datetimes() {
        let target = ExDate::DateTimes(
            AnyForm::Local,
            vec![
                (d("20180101"), t("120000")),
                (d("20180102"), t("110000")),
                (d("20180103"), t("100000")),
                (d("20180105"), t("090000")),
                (d("20180110"), t("080000")),
            ],
        );

        let expected = ics! {"
            :20180101T120000,20180102T110000,20180103T100000,20180105T090000,20180110T0
             80000
        "};

        assert_eq!(expected, target.to_string(Property));
        assert_trip!(expected, ExDate as Property);
    }

    #[test]
    fn utc_datetimes() {
        let target = ExDate::DateTimes(
            AnyForm::Utc,
            vec![
                (d("20180101"), t("120000")),
                (d("20180102"), t("110000")),
                (d("20180103"), t("100000")),
                (d("20180105"), t("090000")),
                (d("20180110"), t("080000")),
            ],
        );

        let expected = ics! {"
            :20180101T120000Z,20180102T110000Z,20180103T100000Z,20180105T090000Z,201801
             10T080000Z
        "};

        assert_eq!(expected, target.to_string(Property));
        assert_trip!(expected, ExDate as Property);
    }

    #[test]
    fn tz_datetimes() {
        let target = ExDate::DateTimes(
            AnyForm::Tz(TzId::from("Europe/Vatican")),
            vec![
                (d("20180101"), t("120000")),
                (d("20180102"), t("110000")),
                (d("20180103"), t("100000")),
                (d("20180105"), t("090000")),
                (d("20180110"), t("080000")),
            ],
        );

        let expected = ics! {"
            ;TZID=Europe/Vatican:20180101T120000,20180102T110000,20180103T100000,201801
             05T090000,20180110T080000
        "};

        assert_eq!(expected, target.to_string(Property));
        assert_trip!(expected, ExDate as Property);
    }
}
