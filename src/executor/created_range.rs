//! A range type a schema created, between the binary form it travels in and
//! the text form its [`Value`] is: the `Value::String` the
//! [`CreatedRange`](crate::CreatedRange) newtype naming it converts into.
//!
//! tokio-postgres reports such a type as a range over its subtype and knows
//! nothing else of it, so its bounds are read and written through the Rust
//! type that subtype reads as, one of [`RangeSubtype`]'s, whose `Display` and
//! `FromStr` are the text form. A built-in range has a `Value::Range` of its
//! own and is not read here.

use jiff::{
    Timestamp,
    civil::{Date, DateTime, Time},
};
use pgorm_query::{Range, RangeSubtype, Value};
use rust_decimal::Decimal;
use tokio_postgres::types::{FromSql, Kind, Type};

use super::query::range_from_sql;

type WireError = Box<dyn std::error::Error + Sync + Send>;

/// A range's binary form, read against its subtype, as its text form.
type ToText = fn(&Type, &[u8]) -> Result<String, WireError>;

/// A range's text form as the range it spells, `None` for text spelling none.
type FromText = fn(&str) -> Option<Range<Value>>;

/// Whether `ty` is a range type a schema created: a range, and none of the
/// six built in, each of which has a `RangeType` and so a `Value::Range`.
// [spec:pgorm:def:sql.value.created-range+3]
pub(crate) fn is_created_range(ty: &Type) -> bool {
    matches!(ty.kind(), Kind::Range(_))
        && !matches!(
            *ty,
            Type::INT4_RANGE
                | Type::INT8_RANGE
                | Type::NUM_RANGE
                | Type::DATE_RANGE
                | Type::TS_RANGE
                | Type::TSTZ_RANGE
        )
}

/// A range type a schema created over a subtype a `RangeSubtype` reads: its
/// subtype, and the two halves of the text form through that Rust type.
#[derive(Clone, Copy)]
pub(crate) struct CreatedRangeCodec<'a> {
    subtype: &'a Type,
    text: ToText,
    bounds: FromText,
}

impl<'a> CreatedRangeCodec<'a> {
    /// The codec of `ty`, a range type a schema created; an error naming its
    /// subtype when no `RangeSubtype` reads that, a domain being read as the
    /// type it is over.
    // [spec:pgorm:def:sql.value.created-range+3]
    pub(crate) fn of(ty: &'a Type) -> Result<Self, WireError> {
        let Kind::Range(subtype) = ty.kind() else {
            return Err(format!("`{ty}` is not a range type").into());
        };
        let mut base = subtype;
        while let Kind::Domain(over) = base.kind() {
            base = over;
        }
        let (text, bounds) = match *base {
            Type::INT2 => halves::<i16>(),
            Type::INT4 => halves::<i32>(),
            Type::INT8 => halves::<i64>(),
            Type::FLOAT4 => halves::<f32>(),
            Type::FLOAT8 => halves::<f64>(),
            Type::NUMERIC => halves::<Decimal>(),
            Type::TEXT | Type::VARCHAR | Type::BPCHAR => halves::<String>(),
            Type::DATE => halves::<Date>(),
            Type::TIME => halves::<Time>(),
            Type::TIMESTAMP => halves::<DateTime>(),
            Type::TIMESTAMPTZ => halves::<Timestamp>(),
            #[cfg(feature = "with-uuid")]
            Type::UUID => halves::<uuid::Uuid>(),
            _ => {
                return Err(format!(
                    "a range type a schema created over `{subtype}` has no value: \
                     no `RangeSubtype` reads `{subtype}`"
                )
                .into());
            }
        };
        Ok(Self {
            subtype: base,
            text,
            bounds,
        })
    }
}

/// A field of `ty`, a range type a schema created, as its text form: the
/// binary form read through the subtype's codec, then written as `Display`
/// writes the range.
// [spec:pgorm:req:conn.copy.rows+2]
pub(crate) fn created_range_text(ty: &Type, raw: &[u8]) -> Result<String, WireError> {
    let codec = CreatedRangeCodec::of(ty)?;
    (codec.text)(codec.subtype, raw)
}

/// The range `text` spells in `ty`, a range type a schema created, each bound
/// the `Value` of the subtype's Rust type; an error for text that spells no
/// range of that subtype.
// [spec:pgorm:req:exec.cursor.binding-range+2]
pub(crate) fn created_range_bounds(ty: &Type, text: &str) -> Result<Range<Value>, WireError> {
    let codec = CreatedRangeCodec::of(ty)?;
    (codec.bounds)(text)
        .ok_or_else(|| format!("`{text}` is the text of no range of `{}`", codec.subtype).into())
}

/// The two halves of the text form for ranges over `T`.
fn halves<T>() -> (ToText, FromText)
where
    T: RangeSubtype + for<'a> FromSql<'a>,
{
    (
        |subtype, raw| Ok(range_from_sql::<T>(subtype, raw)?.to_string()),
        |text| Some(text.parse::<Range<T>>().ok()?.map(Into::into)),
    )
}
