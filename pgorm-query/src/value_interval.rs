//! PostgreSQL's `interval` as a value: three independently signed fields.

use std::{error::Error, fmt};

use postgres_types::{FromSql, Kind, Type};

use crate::{
    ArrayType, ColumnType, IntervalSpec, Nullable, Value, ValueType, ValueTypeError,
    value::with_array::NotU8,
};

type CodecError = Box<dyn Error + Sync + Send>;

/// An `interval`: months, days and microseconds, each with its own sign.
///
/// The three fields are kept apart because PostgreSQL keeps them apart: a
/// month has no fixed number of days and a day no fixed number of
/// microseconds across a daylight-saving change, so `1 mon` is not `30 days`
/// and `1 mon -2 days` is a value of its own. Nothing here normalises one
/// field into another, and every triple is a value the server stores.
///
/// Two triples are PostgreSQL's infinities rather than finite values: every
/// field at its maximum is `infinity`, and every field at its minimum is
/// `-infinity`.
///
/// Equality and hashing are field by field, so `1 mon` and `30 days` are
/// different values here, where PostgreSQL's `=` on `interval` calls them
/// equal.
// [spec:pgorm:def:sql.value.interval]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Interval {
    pub months: i32,
    pub days: i32,
    pub microseconds: i64,
}

impl Interval {
    /// The interval of these three fields, taken as they are.
    // [spec:pgorm:def:sql.value.interval]
    pub const fn new(months: i32, days: i32, microseconds: i64) -> Self {
        Self {
            months,
            days,
            microseconds,
        }
    }

    /// Whether this is PostgreSQL's `infinity`, every field at its maximum.
    // [spec:pgorm:def:sql.value.interval]
    pub const fn is_infinity(&self) -> bool {
        self.months == i32::MAX && self.days == i32::MAX && self.microseconds == i64::MAX
    }

    /// Whether this is PostgreSQL's `-infinity`, every field at its minimum.
    // [spec:pgorm:def:sql.value.interval]
    pub const fn is_neg_infinity(&self) -> bool {
        self.months == i32::MIN && self.days == i32::MIN && self.microseconds == i64::MIN
    }

    /// The binary form, `interval_send`'s: the microseconds, then the days,
    /// then the months, each big-endian. [`FromSql`] reads the same layout.
    // [spec:pgorm:def:sql.value.interval]
    pub fn to_binary(&self) -> [u8; 16] {
        let mut out = [0; 16];
        out[..8].copy_from_slice(&self.microseconds.to_be_bytes());
        out[8..12].copy_from_slice(&self.days.to_be_bytes());
        out[12..].copy_from_slice(&self.months.to_be_bytes());
        out
    }
}

/// The text PostgreSQL's interval input reads back as this exact value:
/// `infinity` or `-infinity` for the two infinite triples, and otherwise each
/// nonzero field as a signed integer and its unit, `+1 months -2 days +3000004
/// microseconds`, or `+0 days` when every field is zero.
///
/// Every field carries an explicit sign because of `IntervalStyle`: under
/// `sql_standard` the server applies a leading `-` to every field that follows
/// it unless another field is signed too, so `-1 days 2 hours` would be read as
/// `-1 days -2 hours`. Microseconds are written as an integer, so no field
/// passes through a fraction the server would have to round. The infinite
/// triples have to be spelled as words: the server's input refuses to compute
/// an infinity from finite fields.
// [spec:pgorm:sem:sql.value.interval-text]
impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_infinity() {
            return f.write_str("infinity");
        }
        if self.is_neg_infinity() {
            return f.write_str("-infinity");
        }
        let fields = [
            (i64::from(self.months), "months"),
            (i64::from(self.days), "days"),
            (self.microseconds, "microseconds"),
        ];
        let mut written = false;
        for (value, unit) in fields.into_iter().filter(|(value, _)| *value != 0) {
            if written {
                f.write_str(" ")?;
            }
            write!(f, "{value:+} {unit}")?;
            written = true;
        }
        if !written {
            f.write_str("+0 days")?;
        }
        Ok(())
    }
}

/// Whether `ty` is `interval`, or a domain over it, which is sent in the
/// representation of the type it is built over.
fn is_interval(ty: &Type) -> bool {
    let mut ty = ty;
    while let Kind::Domain(base) = ty.kind() {
        ty = base;
    }
    *ty == Type::INTERVAL
}

/// The binary form [`Interval::to_binary`] writes.
// [spec:pgorm:def:sql.value.interval]
impl<'a> FromSql<'a> for Interval {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Self, CodecError> {
        let raw: &[u8; 16] = raw
            .try_into()
            .map_err(|_| format!("an interval is sixteen bytes, received {}", raw.len()))?;
        let (microseconds, rest) = raw.split_at(8);
        let (days, months) = rest.split_at(4);
        Ok(Self {
            microseconds: i64::from_be_bytes(microseconds.try_into()?),
            days: i32::from_be_bytes(days.try_into()?),
            months: i32::from_be_bytes(months.try_into()?),
        })
    }

    fn accepts(ty: &Type) -> bool {
        is_interval(ty)
    }
}

// [spec:pgorm:def:sql.value.interval]
impl From<Interval> for Value {
    fn from(x: Interval) -> Value {
        Value::Interval(Some(Box::new(x)))
    }
}

// [spec:pgorm:def:sql.value.interval]
impl Nullable for Interval {
    fn null() -> Value {
        Value::Interval(None)
    }
}

// [spec:pgorm:def:sql.value.interval]
impl ValueType for Interval {
    fn try_from(v: Value) -> Result<Self, ValueTypeError> {
        match v {
            Value::Interval(Some(x)) => Ok(*x),
            _ => Err(ValueTypeError),
        }
    }

    fn type_name() -> String {
        "Interval".to_owned()
    }

    fn array_type() -> ArrayType {
        ArrayType::Interval
    }

    fn column_type() -> ColumnType {
        ColumnType::Interval(IntervalSpec::Any(None))
    }
}

// [spec:pgorm:def:sql.value.interval]
impl NotU8 for Interval {}

impl Value {
    pub fn is_interval(&self) -> bool {
        matches!(self, Self::Interval(_))
    }

    /// The payload of a non-NULL [`Value::Interval`]; `None` otherwise.
    pub fn as_ref_interval(&self) -> Option<&Interval> {
        match self {
            Self::Interval(v) => v.as_deref(),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // [spec:pgorm:def:sql.value.interval/test]
    #[test]
    fn binary_form_is_microseconds_days_months() -> Result<(), CodecError> {
        let interval = Interval::new(1, -2, 3_000_004);
        let bytes = interval.to_binary();
        let mut expected = 3_000_004i64.to_be_bytes().to_vec();
        expected.extend_from_slice(&(-2i32).to_be_bytes());
        expected.extend_from_slice(&1i32.to_be_bytes());
        assert_eq!(bytes.to_vec(), expected);
        assert_eq!(Interval::from_sql(&Type::INTERVAL, &bytes)?, interval);
        Ok(())
    }

    // [spec:pgorm:def:sql.value.interval/test]
    #[test]
    fn binary_form_round_trips_every_extreme() -> Result<(), CodecError> {
        for interval in [
            Interval::default(),
            Interval::new(i32::MAX, i32::MAX, i64::MAX),
            Interval::new(i32::MIN, i32::MIN, i64::MIN),
            Interval::new(i32::MIN, i32::MAX, -1),
            Interval::new(14, -3, -(4 * 3_600_000_000 + 1)),
        ] {
            assert_eq!(
                Interval::from_sql(&Type::INTERVAL, &interval.to_binary())?,
                interval
            );
        }
        Ok(())
    }

    // [spec:pgorm:def:sql.value.interval/test]
    #[test]
    fn codec_refuses_other_types_and_widths() {
        assert!(!<Interval as FromSql>::accepts(&Type::INT8));
        assert!(!<Interval as FromSql>::accepts(&Type::TEXT));
        assert!(Interval::from_sql(&Type::INTERVAL, &[0; 15]).is_err());
        assert!(Interval::from_sql(&Type::INTERVAL, &[0; 17]).is_err());

        let domain = Type::new(
            "span".to_owned(),
            16_384,
            Kind::Domain(Type::INTERVAL),
            "public".to_owned(),
        );
        assert!(<Interval as FromSql>::accepts(&domain));
        assert!(is_interval(&domain));
    }

    // [spec:pgorm:sem:sql.value.interval-text/test]
    #[test]
    fn text_signs_every_nonzero_field() {
        let text =
            |months, days, microseconds| Interval::new(months, days, microseconds).to_string();
        assert_eq!(
            text(1, -2, 3_000_004),
            "+1 months -2 days +3000004 microseconds"
        );
        assert_eq!(text(0, 0, 0), "+0 days");
        assert_eq!(text(-1, 0, 0), "-1 months");
        assert_eq!(text(0, 0, -1), "-1 microseconds");
        assert_eq!(text(0, 3, 0), "+3 days");
        assert_eq!(text(i32::MAX, i32::MAX, i64::MAX), "infinity");
        assert_eq!(text(i32::MIN, i32::MIN, i64::MIN), "-infinity");
        assert_eq!(
            text(i32::MAX, i32::MAX, i64::MAX - 1),
            "+2147483647 months +2147483647 days +9223372036854775806 microseconds"
        );
        assert_eq!(
            text(i32::MIN, i32::MIN, 0),
            "-2147483648 months -2147483648 days"
        );
    }

    // [spec:pgorm:def:sql.value.interval/test]
    #[test]
    fn converts_into_and_out_of_value() {
        let interval = Interval::new(1, -2, 3);
        let value = Value::from(interval);
        assert_eq!(value, Value::Interval(Some(Box::new(interval))));
        assert_eq!(value.as_ref_interval(), Some(&interval));
        assert!(value.is_interval());
        assert_eq!(
            <Interval as ValueType>::try_from(value).ok(),
            Some(interval)
        );
        assert_eq!(Value::from(None::<Interval>), Value::Interval(None));
        assert!(<Interval as ValueType>::try_from(Value::Interval(None)).is_err());
        assert!(<Interval as ValueType>::try_from(Value::BigInt(Some(1))).is_err());
        assert_eq!(
            <Interval as ValueType>::column_type(),
            ColumnType::Interval(IntervalSpec::Any(None))
        );
        assert_eq!(
            Value::from(vec![interval]),
            Value::Array(
                ArrayType::Interval,
                Some(Box::new(vec![Value::from(interval)]))
            )
        );
        assert_eq!(
            <Vec<Interval> as ValueType>::try_from(Value::from(vec![interval])).ok(),
            Some(vec![interval])
        );
    }

    // [spec:pgorm:def:sql.value.interval/test]
    #[test]
    fn equality_is_field_by_field() {
        assert_ne!(
            Value::from(Interval::new(1, 0, 0)),
            Value::from(Interval::new(0, 30, 0))
        );
        assert_ne!(
            Value::from(Interval::new(0, 1, 0)),
            Value::from(Interval::new(0, 0, 86_400_000_000))
        );
        assert_ne!(Value::Interval(None), Value::BigInt(None));
    }
}
