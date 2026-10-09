//! pgorm's values as the binding holds them: a `pgorm_query::Value` with the
//! identity plain JavaScript cannot carry — an enum's type, a created range's
//! type and subtype, an array's element kind — and the one kind PostgreSQL has
//! that pgorm's `Value` does not, the interval.

pub(crate) mod created;
pub(crate) mod interval;
pub(crate) mod json;
pub(crate) mod read;
mod value;
pub(crate) mod write;

use pgorm::pgorm_query::{ArrayType, RangeType, Value};

pub(crate) use interval::Interval;
pub(crate) use value::export;

/// A PostgreSQL type named by identifier: an enum, or a range type a schema
/// created.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TypeName {
    pub(crate) name: String,
    pub(crate) schema: Option<String>,
}

/// A range or multirange type a schema created: its name, the value kind of
/// its subtype, and which of the two it is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CreatedKind {
    pub(crate) name: TypeName,
    pub(crate) subtype: ArrayType,
    pub(crate) multirange: bool,
}

/// A kind with no type name: one of pgorm's `Value` variants, or an interval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Scalar {
    Value(ArrayType),
    Interval,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Tag {
    Scalar(Scalar),
    Enum(TypeName),
    Created(CreatedKind),
    Array(Box<Tag>),
}

/// What a value holds. An enum label and a created range's text form are
/// `Value::String`, as pgorm's own `DeriveActiveEnum` and
/// `DeriveCreatedRange` types convert into.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Datum {
    Value(Value),
    Interval(Option<Interval>),
    Intervals(Option<Vec<Option<Interval>>>),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Tagged {
    pub(crate) datum: Datum,
    pub(crate) tag: Tag,
}

impl neon::types::Finalize for Tagged {}

pub(crate) const RANGE_TYPES: [RangeType; 6] = [
    RangeType::Int4,
    RangeType::Int8,
    RangeType::Numeric,
    RangeType::Date,
    RangeType::Timestamp,
    RangeType::TimestampTz,
];

/// The scalar kind a built-in range type's bounds are.
pub(crate) fn range_element(range: RangeType) -> ArrayType {
    match range {
        RangeType::Int4 => ArrayType::Int,
        RangeType::Int8 => ArrayType::BigInt,
        RangeType::Numeric => ArrayType::Decimal,
        RangeType::Date => ArrayType::Date,
        RangeType::Timestamp => ArrayType::DateTime,
        RangeType::TimestampTz => ArrayType::DateTimeWithTimeZone,
    }
}

/// The value kinds a created range can range over: pgorm's `RangeSubtype`.
pub(crate) const CREATED_SUBTYPES: [ArrayType; 12] = [
    ArrayType::SmallInt,
    ArrayType::Int,
    ArrayType::BigInt,
    ArrayType::Float,
    ArrayType::Double,
    ArrayType::Decimal,
    ArrayType::String,
    ArrayType::Date,
    ArrayType::Time,
    ArrayType::DateTime,
    ArrayType::DateTimeWithTimeZone,
    ArrayType::Uuid,
];

macro_rules! scalar_kinds {
    ($( $variant:ident => $name:literal ),+ $(,)?) => {
        /// A kind's name, spelled as pgorm-python spells it: `i64`,
        /// `datetime_utc`, `int4range`.
        pub(crate) fn scalar_name(kind: &ArrayType) -> &'static str {
            match kind {
                $(ArrayType::$variant => $name,)+
                ArrayType::Range(ty) => ty.range_type_name(),
                ArrayType::Multirange(ty) => ty.multirange_type_name(),
            }
        }

        pub(crate) fn parse_scalar(name: &str) -> Option<Scalar> {
            match name {
                $($name => Some(Scalar::Value(ArrayType::$variant)),)+
                "interval" => Some(Scalar::Interval),
                _ => RANGE_TYPES.into_iter().find_map(|range| {
                    if name == range.range_type_name() {
                        Some(Scalar::Value(ArrayType::Range(range)))
                    } else if name == range.multirange_type_name() {
                        Some(Scalar::Value(ArrayType::Multirange(range)))
                    } else {
                        None
                    }
                }),
            }
        }

        pub(crate) fn scalar_null(kind: &ArrayType) -> Value {
            match kind {
                $(ArrayType::$variant => Value::$variant(None),)+
                ArrayType::Range(ty) => Value::Range(*ty, None),
                ArrayType::Multirange(ty) => Value::Multirange(*ty, None),
            }
        }

        /// The kind a pgorm value declares itself.
        pub(crate) fn value_tag(value: &Value) -> Tag {
            match value {
                $(Value::$variant(_) => Tag::Scalar(Scalar::Value(ArrayType::$variant)),)+
                Value::Array(kind, _) => {
                    Tag::Array(Box::new(Tag::Scalar(Scalar::Value(kind.clone()))))
                }
                Value::Range(ty, _) => Tag::Scalar(Scalar::Value(ArrayType::Range(*ty))),
                Value::Multirange(ty, _) => {
                    Tag::Scalar(Scalar::Value(ArrayType::Multirange(*ty)))
                }
            }
        }

        pub(crate) fn value_is_null(value: &Value) -> bool {
            match value {
                $(Value::$variant(value) => value.is_none(),)+
                Value::Array(_, value) => value.is_none(),
                Value::Range(_, value) => value.is_none(),
                Value::Multirange(_, value) => value.is_none(),
            }
        }
    };
}

scalar_kinds! {
    Bool => "bool", TinyInt => "i8", SmallInt => "i16", Int => "i32",
    BigInt => "i64", Unsigned => "u32", BigUnsigned => "u64",
    Float => "f32", Double => "f64", String => "text", Char => "char",
    Bytes => "bytes", Json => "json", Decimal => "decimal", Uuid => "uuid",
    Date => "date", Time => "time", DateTime => "datetime",
    DateTimeWithTimeZone => "datetime_utc", IpNetwork => "ipnetwork",
    MacAddress => "mac_address", Vector => "vector",
}

impl Scalar {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Value(kind) => scalar_name(kind),
            Self::Interval => "interval",
        }
    }
}

impl Tag {
    /// The kind's name: a scalar's own, or `enum`, `created_range`,
    /// `created_multirange` or `array`.
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Scalar(scalar) => scalar.name(),
            Self::Enum(_) => "enum",
            Self::Created(kind) if kind.multirange => "created_multirange",
            Self::Created(_) => "created_range",
            Self::Array(_) => "array",
        }
    }
}

impl Tagged {
    /// SQL NULL of a scalar kind.
    pub(crate) fn null(tag: Tag) -> Self {
        let datum = match &tag {
            Tag::Scalar(Scalar::Value(kind)) => Datum::Value(scalar_null(kind)),
            Tag::Scalar(Scalar::Interval) => Datum::Interval(None),
            Tag::Enum(_) | Tag::Created(_) => Datum::Value(Value::String(None)),
            Tag::Array(element) => match element.as_ref() {
                Tag::Scalar(Scalar::Interval) => Datum::Intervals(None),
                Tag::Scalar(Scalar::Value(kind)) => Datum::Value(Value::Array(kind.clone(), None)),
                _ => Datum::Value(Value::Array(ArrayType::String, None)),
            },
        };
        Self { datum, tag }
    }

    /// A pgorm value, tagged with the kind it declares itself.
    pub(crate) fn value(value: Value) -> Self {
        Self {
            tag: value_tag(&value),
            datum: Datum::Value(value),
        }
    }

    pub(crate) fn is_null(&self) -> bool {
        match &self.datum {
            Datum::Value(value) => value_is_null(value),
            Datum::Interval(interval) => interval.is_none(),
            Datum::Intervals(intervals) => intervals.is_none(),
        }
    }
}
