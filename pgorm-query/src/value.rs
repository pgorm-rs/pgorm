//! Container for all SQL value types.

use std::{borrow::Cow, hash::Hash};

use serde_json::Value as Json;

use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};

use rust_decimal::Decimal;

use uuid::Uuid;

pub use ipnetwork::IpNetwork;

use std::net::IpAddr;

pub use mac_address::MacAddress;

pub use pgvector::Vector;

use crate::{ColumnType, QueryBuilder, StringLen};

/// [`Value`] types variant for Postgres array
// [spec:pgorm:def:sql.value.array+4]
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ArrayType {
    Bool,
    TinyInt,
    SmallInt,
    Int,
    BigInt,
    Unsigned,
    BigUnsigned,
    Float,
    Double,
    String,
    Char,
    Bytes,

    Json,

    ChronoDate,

    ChronoTime,

    ChronoDateTime,

    ChronoDateTimeUtc,

    ChronoDateTimeLocal,

    ChronoDateTimeWithTimeZone,

    Uuid,

    Decimal,

    IpNetwork,

    MacAddress,

    Vector,
}

/// Value variants
///
/// We want the inner Value to be exactly 1 pointer sized, so anything larger should be boxed.
///
/// The float-carrying variants (`Float`, `Double` and `Vector`) compare by bit pattern
/// rather than by IEEE equality, which is what lets `PartialEq`, `Eq` and `Hash` agree:
/// `NaN` equals itself, and `0.0` and `-0.0` are distinct values.
// [spec:pgorm:def:sql.value+2]
#[derive(Clone, Debug)]
pub enum Value {
    Bool(Option<bool>),
    TinyInt(Option<i8>),
    SmallInt(Option<i16>),
    Int(Option<i32>),
    BigInt(Option<i64>),
    Unsigned(Option<u32>),
    BigUnsigned(Option<u64>),
    Float(Option<f32>),
    Double(Option<f64>),
    String(Option<Box<String>>),
    Char(Option<char>),

    #[allow(clippy::box_collection)]
    Bytes(Option<Box<Vec<u8>>>),

    Json(Option<Box<Json>>),

    ChronoDate(Option<Box<NaiveDate>>),

    ChronoTime(Option<Box<NaiveTime>>),

    ChronoDateTime(Option<Box<NaiveDateTime>>),

    ChronoDateTimeUtc(Option<Box<DateTime<Utc>>>),

    ChronoDateTimeLocal(Option<Box<DateTime<Local>>>),

    ChronoDateTimeWithTimeZone(Option<Box<DateTime<FixedOffset>>>),

    Uuid(Option<Box<Uuid>>),

    Decimal(Option<Box<Decimal>>),

    Array(ArrayType, Option<Box<Vec<Value>>>),

    Vector(Option<Box<pgvector::Vector>>),

    IpNetwork(Option<Box<IpNetwork>>),

    MacAddress(Option<Box<MacAddress>>),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", QueryBuilder.value_to_string(self))
    }
}

// [spec:pgorm:def:sql.value.value-type+3]
pub trait ValueType: Sized {
    fn try_from(v: Value) -> Result<Self, ValueTypeError>;

    fn type_name() -> String;

    fn array_type() -> ArrayType;

    fn column_type() -> ColumnType;
}

#[derive(Debug)]
pub struct ValueTypeError;

impl std::error::Error for ValueTypeError {}

impl std::fmt::Display for ValueTypeError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Value type mismatch")
    }
}

// [spec:pgorm:def:sql.value.tuple+2]
#[derive(Clone, Debug, PartialEq)]
pub struct Values(pub Vec<Value>);

// [spec:pgorm:def:sql.value.tuple+2]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ValueTuple {
    One(Value),
    Two(Value, Value),
    Three(Value, Value, Value),
    Many(Vec<Value>),
}

/// The shape of a [`ValueTuple`], carrying its arity but none of its values.
// [spec:pgorm:def:sql.value.tuple+2]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ValueTupleShape {
    /// A [`ValueTuple::One`].
    One,
    /// A [`ValueTuple::Two`].
    Two,
    /// A [`ValueTuple::Three`].
    Three,
    /// A [`ValueTuple::Many`] holding this many values.
    Many(usize),
}

impl std::fmt::Display for ValueTupleShape {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::One => f.write_str("ValueTuple::One"),
            Self::Two => f.write_str("ValueTuple::Two"),
            Self::Three => f.write_str("ValueTuple::Three"),
            Self::Many(len) => write!(f, "ValueTuple::Many with length of {len}"),
        }
    }
}

/// Why a [`ValueTuple`] could not be rebuilt into a typed tuple.
// [spec:pgorm:def:sql.value.tuple+2]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueTupleError {
    /// The tuple's shape is not the one the target type requires.
    Arity {
        /// Shape the target type requires.
        expected: ValueTupleShape,
        /// Shape actually received.
        actual: ValueTupleShape,
    },
    /// A value could not be converted to the type the target holds there.
    Element {
        /// Zero-based position of the value within the tuple.
        position: usize,
        /// Name of the type expected at that position.
        expected: String,
    },
}

impl std::error::Error for ValueTupleError {}

impl std::fmt::Display for ValueTupleError {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        match self {
            Self::Arity { expected, actual } => {
                write!(f, "expected {expected}, received {actual}")
            }
            Self::Element { position, expected } => {
                write!(
                    f,
                    "value at position {position} is not a valid `{expected}`"
                )
            }
        }
    }
}

impl ValueTuple {
    /// The shape of this tuple, without its values.
    // [spec:pgorm:def:sql.value.tuple+2]
    pub fn shape(&self) -> ValueTupleShape {
        match self {
            Self::One(_) => ValueTupleShape::One,
            Self::Two(_, _) => ValueTupleShape::Two,
            Self::Three(_, _, _) => ValueTupleShape::Three,
            Self::Many(vec) => ValueTupleShape::Many(vec.len()),
        }
    }
}

pub trait IntoValueTuple {
    fn into_value_tuple(self) -> ValueTuple;
}

/// The fallible inverse of [`IntoValueTuple`].
pub trait TryFromValueTuple: Sized {
    /// Rebuild the typed tuple, reporting the shape or the position that did not match.
    fn try_from_value_tuple<I>(i: I) -> Result<Self, ValueTupleError>
    where
        I: IntoValueTuple;
}

pub trait Nullable {
    fn null() -> Value;
}

impl Value {
    /// Name of the Postgres type this value is bound as, for pinning a
    /// placeholder whose type would otherwise be inferred from context.
    ///
    /// `None` means the variant has no single type to pin it to: `Json` binds
    /// as either `json` or `jsonb`, and `Vector` binds as an extension type
    /// whose name is not guaranteed to resolve in the current search path.
    // [spec:pgorm:req:sql.render.cast-param-type]
    pub fn source_type_name(&self) -> Option<Cow<'static, str>> {
        match self {
            Self::Json(_) | Self::Vector(_) => None,
            Self::Array(ty, _) => ty
                .source_type_name()
                .map(|name| Cow::Owned(format!("{name}[]"))),
            Self::Bool(_) => Some(Cow::Borrowed("bool")),
            Self::TinyInt(_) => Some(Cow::Borrowed("int2")),
            Self::SmallInt(_) => Some(Cow::Borrowed("int2")),
            Self::Int(_) => Some(Cow::Borrowed("int4")),
            Self::BigInt(_) => Some(Cow::Borrowed("int8")),
            Self::Unsigned(_) => Some(Cow::Borrowed("int8")),
            Self::BigUnsigned(_) => Some(Cow::Borrowed("int8")),
            Self::Float(_) => Some(Cow::Borrowed("float4")),
            Self::Double(_) => Some(Cow::Borrowed("float8")),
            Self::String(_) => Some(Cow::Borrowed("text")),
            Self::Char(_) => Some(Cow::Borrowed("text")),
            Self::Bytes(_) => Some(Cow::Borrowed("bytea")),
            Self::ChronoDate(_) => Some(Cow::Borrowed("date")),
            Self::ChronoTime(_) => Some(Cow::Borrowed("time")),
            Self::ChronoDateTime(_) => Some(Cow::Borrowed("timestamp")),
            Self::ChronoDateTimeUtc(_) => Some(Cow::Borrowed("timestamptz")),
            Self::ChronoDateTimeLocal(_) => Some(Cow::Borrowed("timestamptz")),
            Self::ChronoDateTimeWithTimeZone(_) => Some(Cow::Borrowed("timestamptz")),
            Self::Uuid(_) => Some(Cow::Borrowed("uuid")),
            Self::Decimal(_) => Some(Cow::Borrowed("numeric")),
            Self::IpNetwork(_) => Some(Cow::Borrowed("inet")),
            Self::MacAddress(_) => Some(Cow::Borrowed("macaddr")),
        }
    }
}

impl ArrayType {
    /// Name of the Postgres type an element of this array is bound as. See
    /// [`Value::source_type_name`].
    // [spec:pgorm:req:sql.render.cast-param-type]
    pub fn source_type_name(&self) -> Option<&'static str> {
        match self {
            Self::Json | Self::Vector => None,
            Self::Bool => Some("bool"),
            Self::TinyInt => Some("int2"),
            Self::SmallInt => Some("int2"),
            Self::Int => Some("int4"),
            Self::BigInt => Some("int8"),
            Self::Unsigned => Some("int8"),
            Self::BigUnsigned => Some("int8"),
            Self::Float => Some("float4"),
            Self::Double => Some("float8"),
            Self::String => Some("text"),
            Self::Char => Some("text"),
            Self::Bytes => Some("bytea"),
            Self::ChronoDate => Some("date"),
            Self::ChronoTime => Some("time"),
            Self::ChronoDateTime => Some("timestamp"),
            Self::ChronoDateTimeUtc => Some("timestamptz"),
            Self::ChronoDateTimeLocal => Some("timestamptz"),
            Self::ChronoDateTimeWithTimeZone => Some("timestamptz"),
            Self::Uuid => Some("uuid"),
            Self::Decimal => Some("numeric"),
            Self::IpNetwork => Some("inet"),
            Self::MacAddress => Some("macaddr"),
        }
    }
}

// [spec:pgorm:def:sql.value.conversions+1]
macro_rules! type_to_value {
    ( $type: ty, $name: ident, $col_type: expr ) => {
        impl From<$type> for Value {
            fn from(x: $type) -> Value {
                Value::$name(Some(x))
            }
        }

        impl Nullable for $type {
            fn null() -> Value {
                Value::$name(None)
            }
        }

        impl ValueType for $type {
            fn try_from(v: Value) -> Result<Self, ValueTypeError> {
                match v {
                    Value::$name(Some(x)) => Ok(x),
                    _ => Err(ValueTypeError),
                }
            }

            fn type_name() -> String {
                stringify!($type).to_owned()
            }

            fn array_type() -> ArrayType {
                ArrayType::$name
            }

            fn column_type() -> ColumnType {
                use ColumnType::*;
                $col_type
            }
        }
    };
}

macro_rules! type_to_box_value {
    ( $type: ty, $name: ident, $col_type: expr ) => {
        impl From<$type> for Value {
            fn from(x: $type) -> Value {
                Value::$name(Some(Box::new(x)))
            }
        }

        impl Nullable for $type {
            fn null() -> Value {
                Value::$name(None)
            }
        }

        impl ValueType for $type {
            fn try_from(v: Value) -> Result<Self, ValueTypeError> {
                match v {
                    Value::$name(Some(x)) => Ok(*x),
                    _ => Err(ValueTypeError),
                }
            }

            fn type_name() -> String {
                stringify!($type).to_owned()
            }

            fn array_type() -> ArrayType {
                ArrayType::$name
            }

            fn column_type() -> ColumnType {
                use ColumnType::*;
                $col_type
            }
        }
    };
}

type_to_value!(bool, Bool, Boolean);
type_to_value!(i8, TinyInt, SmallInteger);
type_to_value!(i16, SmallInt, SmallInteger);
type_to_value!(i32, Int, Integer);
type_to_value!(i64, BigInt, BigInteger);
type_to_value!(u32, Unsigned, BigInteger);
type_to_value!(u64, BigUnsigned, BigInteger);
type_to_value!(f32, Float, Float);
type_to_value!(f64, Double, Double);
type_to_value!(char, Char, Char(None));

impl From<&[u8]> for Value {
    fn from(x: &[u8]) -> Value {
        Value::Bytes(Some(Box::<Vec<u8>>::new(x.into())))
    }
}

impl From<&str> for Value {
    fn from(x: &str) -> Value {
        let string: String = x.into();
        Value::String(Some(Box::new(string)))
    }
}

impl From<&String> for Value {
    fn from(x: &String) -> Value {
        let string: String = x.into();
        Value::String(Some(Box::new(string)))
    }
}

impl Nullable for &str {
    fn null() -> Value {
        Value::String(None)
    }
}

// [spec:pgorm:def:sql.value.conversions+1]
impl<T> From<Option<T>> for Value
where
    T: Into<Value> + Nullable,
{
    fn from(x: Option<T>) -> Value {
        match x {
            Some(v) => v.into(),
            None => T::null(),
        }
    }
}

// [spec:pgorm:def:sql.value.value-type+3]
impl<T> ValueType for Option<T>
where
    T: ValueType + Nullable,
{
    fn try_from(v: Value) -> Result<Self, ValueTypeError> {
        if v == T::null() {
            Ok(None)
        } else {
            Ok(Some(T::try_from(v)?))
        }
    }

    fn type_name() -> String {
        format!("Option<{}>", T::type_name())
    }

    fn array_type() -> ArrayType {
        T::array_type()
    }

    fn column_type() -> ColumnType {
        T::column_type()
    }
}

impl From<Cow<'_, str>> for Value {
    fn from(x: Cow<'_, str>) -> Value {
        x.into_owned().into()
    }
}

impl ValueType for Cow<'_, str> {
    fn try_from(v: Value) -> Result<Self, ValueTypeError> {
        match v {
            Value::String(Some(x)) => Ok((*x).into()),
            _ => Err(ValueTypeError),
        }
    }

    fn type_name() -> String {
        "Cow<str>".into()
    }

    fn array_type() -> ArrayType {
        ArrayType::String
    }

    fn column_type() -> ColumnType {
        ColumnType::String(StringLen::None)
    }
}

type_to_box_value!(Vec<u8>, Bytes, Bytea);
type_to_box_value!(String, String, String(StringLen::None));

mod with_json {
    use super::*;

    type_to_box_value!(Json, Json, Json);
}

mod with_chrono {
    use super::*;
    use chrono::{Local, Offset, Utc};

    type_to_box_value!(NaiveDate, ChronoDate, Date);
    type_to_box_value!(NaiveTime, ChronoTime, Time);
    type_to_box_value!(NaiveDateTime, ChronoDateTime, Timestamp);

    impl From<DateTime<Utc>> for Value {
        fn from(v: DateTime<Utc>) -> Value {
            Value::ChronoDateTimeUtc(Some(Box::new(v)))
        }
    }

    impl From<DateTime<Local>> for Value {
        fn from(v: DateTime<Local>) -> Value {
            Value::ChronoDateTimeLocal(Some(Box::new(v)))
        }
    }

    impl From<DateTime<FixedOffset>> for Value {
        fn from(x: DateTime<FixedOffset>) -> Value {
            let v =
                DateTime::<FixedOffset>::from_naive_utc_and_offset(x.naive_utc(), x.offset().fix());
            Value::ChronoDateTimeWithTimeZone(Some(Box::new(v)))
        }
    }

    impl Nullable for DateTime<Utc> {
        fn null() -> Value {
            Value::ChronoDateTimeUtc(None)
        }
    }

    impl ValueType for DateTime<Utc> {
        fn try_from(v: Value) -> Result<Self, ValueTypeError> {
            match v {
                Value::ChronoDateTimeUtc(Some(x)) => Ok(*x),
                _ => Err(ValueTypeError),
            }
        }

        fn type_name() -> String {
            stringify!(DateTime<Utc>).to_owned()
        }

        fn array_type() -> ArrayType {
            ArrayType::ChronoDateTimeUtc
        }

        fn column_type() -> ColumnType {
            ColumnType::TimestampWithTimeZone
        }
    }

    impl Nullable for DateTime<Local> {
        fn null() -> Value {
            Value::ChronoDateTimeLocal(None)
        }
    }

    impl ValueType for DateTime<Local> {
        fn try_from(v: Value) -> Result<Self, ValueTypeError> {
            match v {
                Value::ChronoDateTimeLocal(Some(x)) => Ok(*x),
                _ => Err(ValueTypeError),
            }
        }

        fn type_name() -> String {
            stringify!(DateTime<Local>).to_owned()
        }

        fn array_type() -> ArrayType {
            ArrayType::ChronoDateTimeLocal
        }

        fn column_type() -> ColumnType {
            ColumnType::TimestampWithTimeZone
        }
    }

    impl Nullable for DateTime<FixedOffset> {
        fn null() -> Value {
            Value::ChronoDateTimeWithTimeZone(None)
        }
    }

    impl ValueType for DateTime<FixedOffset> {
        fn try_from(v: Value) -> Result<Self, ValueTypeError> {
            match v {
                Value::ChronoDateTimeWithTimeZone(Some(x)) => Ok(*x),
                _ => Err(ValueTypeError),
            }
        }

        fn type_name() -> String {
            stringify!(DateTime<FixedOffset>).to_owned()
        }

        fn array_type() -> ArrayType {
            ArrayType::ChronoDateTimeWithTimeZone
        }

        fn column_type() -> ColumnType {
            ColumnType::TimestampWithTimeZone
        }
    }
}

mod with_rust_decimal {
    use super::*;

    type_to_box_value!(Decimal, Decimal, Decimal(None));
}

mod with_uuid {
    use super::*;

    type_to_box_value!(Uuid, Uuid, Uuid);

    macro_rules! fmt_uuid_to_box_value {
        ( $type: ty, $conversion_fn: ident ) => {
            impl From<$type> for Value {
                fn from(x: $type) -> Value {
                    Value::Uuid(Some(Box::new(x.into_uuid())))
                }
            }

            impl Nullable for $type {
                fn null() -> Value {
                    Value::Uuid(None)
                }
            }

            impl ValueType for $type {
                fn try_from(v: Value) -> Result<Self, ValueTypeError> {
                    match v {
                        Value::Uuid(Some(x)) => Ok(x.$conversion_fn()),
                        _ => Err(ValueTypeError),
                    }
                }

                fn type_name() -> String {
                    stringify!($type).to_owned()
                }

                fn array_type() -> ArrayType {
                    ArrayType::Uuid
                }

                fn column_type() -> ColumnType {
                    ColumnType::Uuid
                }
            }
        };
    }

    fmt_uuid_to_box_value!(uuid::fmt::Braced, braced);
    fmt_uuid_to_box_value!(uuid::fmt::Hyphenated, hyphenated);
    fmt_uuid_to_box_value!(uuid::fmt::Simple, simple);
    fmt_uuid_to_box_value!(uuid::fmt::Urn, urn);
}

mod with_ipnetwork {
    use super::*;

    type_to_box_value!(IpNetwork, IpNetwork, Inet);
}

mod with_mac_address {
    use super::*;

    type_to_box_value!(MacAddress, MacAddress, MacAddr);
}

// [spec:pgorm:def:sql.value.array+4]
pub mod with_array {
    use super::*;
    use std::sync::Arc;

    // We only imlement conversion from Vec<T> to Array when T is not u8.
    // This is because for u8's case, there is already conversion to Byte defined above.
    // TODO When negative trait becomes a stable feature, following code can be much shorter.
    pub trait NotU8 {}

    impl NotU8 for bool {}
    impl NotU8 for i8 {}
    impl NotU8 for i16 {}
    impl NotU8 for i32 {}
    impl NotU8 for i64 {}
    impl NotU8 for u32 {}
    impl NotU8 for u64 {}
    impl NotU8 for f32 {}
    impl NotU8 for f64 {}
    impl NotU8 for char {}
    impl NotU8 for String {}
    impl NotU8 for Vec<u8> {}

    // TODO impl<T: NotU8> NotU8 for Option<T> {}

    impl NotU8 for Json {}

    impl NotU8 for NaiveDate {}

    impl NotU8 for NaiveTime {}

    impl NotU8 for NaiveDateTime {}

    impl<Tz> NotU8 for DateTime<Tz> where Tz: chrono::TimeZone {}

    impl NotU8 for Decimal {}

    impl NotU8 for Uuid {}

    impl NotU8 for uuid::fmt::Braced {}

    impl NotU8 for uuid::fmt::Hyphenated {}

    impl NotU8 for uuid::fmt::Simple {}

    impl NotU8 for uuid::fmt::Urn {}

    impl NotU8 for IpNetwork {}

    impl NotU8 for MacAddress {}

    impl<T> From<Vec<T>> for Value
    where
        T: Into<Value> + NotU8 + ValueType,
    {
        fn from(x: Vec<T>) -> Value {
            Value::Array(
                T::array_type(),
                Some(Box::new(x.into_iter().map(|e| e.into()).collect())),
            )
        }
    }

    impl<T> Nullable for Vec<T>
    where
        T: Into<Value> + NotU8 + ValueType,
    {
        fn null() -> Value {
            Value::Array(T::array_type(), None)
        }
    }

    impl<T> ValueType for Vec<T>
    where
        T: NotU8 + ValueType,
    {
        fn try_from(v: Value) -> Result<Self, ValueTypeError> {
            match v {
                Value::Array(ty, Some(v)) if T::array_type() == ty => {
                    v.into_iter().map(T::try_from).collect()
                }
                _ => Err(ValueTypeError),
            }
        }

        fn type_name() -> String {
            stringify!(Vec<T>).to_owned()
        }

        fn array_type() -> ArrayType {
            T::array_type()
        }

        fn column_type() -> ColumnType {
            use ColumnType::*;
            Array(Arc::new(T::column_type()))
        }
    }
}

pub mod with_vector {
    use super::*;

    impl From<pgvector::Vector> for Value {
        fn from(x: pgvector::Vector) -> Value {
            Value::Vector(Some(Box::new(x)))
        }
    }

    impl Nullable for pgvector::Vector {
        fn null() -> Value {
            Value::Vector(None)
        }
    }

    impl ValueType for pgvector::Vector {
        fn try_from(v: Value) -> Result<Self, ValueTypeError> {
            match v {
                Value::Vector(Some(x)) => Ok(*x),
                _ => Err(ValueTypeError),
            }
        }

        fn type_name() -> String {
            stringify!(Vector).to_owned()
        }

        fn array_type() -> ArrayType {
            ArrayType::Vector
        }

        fn column_type() -> ColumnType {
            ColumnType::Vector(None)
        }
    }
}

// [spec:pgorm:sem:sql.value.accessor-panics+2]
impl Value {
    pub fn is_json(&self) -> bool {
        matches!(self, Self::Json(_))
    }

    /// The payload of a non-NULL [`Value::Json`]; `None` for a NULL of that
    /// variant and for every other variant alike. Discriminate with
    /// [`Value::is_json`].
    pub fn as_ref_json(&self) -> Option<&Json> {
        match self {
            Self::Json(v) => v.as_deref(),
            _ => None,
        }
    }
}

impl Value {
    pub fn is_chrono_date(&self) -> bool {
        matches!(self, Self::ChronoDate(_))
    }

    /// The payload of a non-NULL [`Value::ChronoDate`]; `None` otherwise.
    pub fn as_ref_chrono_date(&self) -> Option<&NaiveDate> {
        match self {
            Self::ChronoDate(v) => v.as_deref(),
            _ => None,
        }
    }
}

impl Value {
    pub fn is_chrono_time(&self) -> bool {
        matches!(self, Self::ChronoTime(_))
    }

    /// The payload of a non-NULL [`Value::ChronoTime`]; `None` otherwise.
    pub fn as_ref_chrono_time(&self) -> Option<&NaiveTime> {
        match self {
            Self::ChronoTime(v) => v.as_deref(),
            _ => None,
        }
    }
}

impl Value {
    pub fn is_chrono_date_time(&self) -> bool {
        matches!(self, Self::ChronoDateTime(_))
    }

    /// The payload of a non-NULL [`Value::ChronoDateTime`]; `None` otherwise.
    pub fn as_ref_chrono_date_time(&self) -> Option<&NaiveDateTime> {
        match self {
            Self::ChronoDateTime(v) => v.as_deref(),
            _ => None,
        }
    }
}

impl Value {
    pub fn is_chrono_date_time_utc(&self) -> bool {
        matches!(self, Self::ChronoDateTimeUtc(_))
    }

    /// The payload of a non-NULL [`Value::ChronoDateTimeUtc`]; `None` otherwise.
    pub fn as_ref_chrono_date_time_utc(&self) -> Option<&DateTime<Utc>> {
        match self {
            Self::ChronoDateTimeUtc(v) => v.as_deref(),
            _ => None,
        }
    }
}

impl Value {
    pub fn is_chrono_date_time_local(&self) -> bool {
        matches!(self, Self::ChronoDateTimeLocal(_))
    }

    /// The payload of a non-NULL [`Value::ChronoDateTimeLocal`]; `None` otherwise.
    pub fn as_ref_chrono_date_time_local(&self) -> Option<&DateTime<Local>> {
        match self {
            Self::ChronoDateTimeLocal(v) => v.as_deref(),
            _ => None,
        }
    }
}

impl Value {
    pub fn is_chrono_date_time_with_time_zone(&self) -> bool {
        matches!(self, Self::ChronoDateTimeWithTimeZone(_))
    }

    /// The payload of a non-NULL [`Value::ChronoDateTimeWithTimeZone`]; `None`
    /// otherwise.
    pub fn as_ref_chrono_date_time_with_time_zone(&self) -> Option<&DateTime<FixedOffset>> {
        match self {
            Self::ChronoDateTimeWithTimeZone(v) => v.as_deref(),
            _ => None,
        }
    }
}

// [spec:pgorm:sem:sql.value.accessor-panics+2]
impl Value {
    /// The UTC-naive form of any non-NULL chrono variant, stringified; `None`
    /// for a NULL chrono variant and for every non-chrono variant alike.
    pub fn chrono_as_naive_utc_in_string(&self) -> Option<String> {
        match self {
            Self::ChronoDate(v) => v.as_ref().map(|v| v.to_string()),
            Self::ChronoTime(v) => v.as_ref().map(|v| v.to_string()),
            Self::ChronoDateTime(v) => v.as_ref().map(|v| v.to_string()),
            Self::ChronoDateTimeUtc(v) => v.as_ref().map(|v| v.naive_utc().to_string()),
            Self::ChronoDateTimeLocal(v) => v.as_ref().map(|v| v.naive_utc().to_string()),
            Self::ChronoDateTimeWithTimeZone(v) => v.as_ref().map(|v| v.naive_utc().to_string()),
            _ => None,
        }
    }
}

impl Value {
    pub fn is_decimal(&self) -> bool {
        matches!(self, Self::Decimal(_))
    }

    /// The payload of a non-NULL [`Value::Decimal`]; `None` otherwise.
    pub fn as_ref_decimal(&self) -> Option<&Decimal> {
        match self {
            Self::Decimal(v) => v.as_deref(),
            _ => None,
        }
    }

    /// The payload of a non-NULL [`Value::Decimal`] as `f64`; `None` otherwise,
    /// and `None` again for a payload that has no `f64` representation.
    pub fn decimal_to_f64(&self) -> Option<f64> {
        use rust_decimal::prelude::ToPrimitive;

        self.as_ref_decimal().and_then(|d| d.to_f64())
    }
}

impl Value {
    pub fn is_uuid(&self) -> bool {
        matches!(self, Self::Uuid(_))
    }

    /// The payload of a non-NULL [`Value::Uuid`]; `None` otherwise.
    pub fn as_ref_uuid(&self) -> Option<&Uuid> {
        match self {
            Self::Uuid(v) => v.as_deref(),
            _ => None,
        }
    }
}

impl Value {
    /// One array value gathered from homogeneous elements.
    ///
    /// The element type tag is read from `V` rather than from the elements, so
    /// an empty iterator still names its element type: an untagged empty array
    /// has no inline spelling PostgreSQL can type, and no element to infer one
    /// from.
    // [spec:pgorm:def:sql.value.array+4]
    pub fn array<V, I>(values: I) -> Self
    where
        V: Into<Value> + ValueType,
        I: IntoIterator<Item = V>,
    {
        Self::Array(
            V::array_type(),
            Some(Box::new(values.into_iter().map(Into::into).collect())),
        )
    }

    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_, _))
    }

    /// The elements of a non-NULL [`Value::Array`], whatever its element tag;
    /// `None` otherwise.
    pub fn as_ref_array(&self) -> Option<&Vec<Value>> {
        match self {
            Self::Array(_, v) => v.as_deref(),
            _ => None,
        }
    }
}

impl Value {
    pub fn is_ipnetwork(&self) -> bool {
        matches!(self, Self::IpNetwork(_))
    }

    /// The payload of a non-NULL [`Value::IpNetwork`]; `None` otherwise.
    pub fn as_ref_ipnetwork(&self) -> Option<&IpNetwork> {
        match self {
            Self::IpNetwork(v) => v.as_deref(),
            _ => None,
        }
    }

    /// The network address of a non-NULL [`Value::IpNetwork`]; `None` otherwise.
    pub fn as_ipaddr(&self) -> Option<IpAddr> {
        match self {
            Self::IpNetwork(v) => v.as_ref().map(|v| v.network()),
            _ => None,
        }
    }
}

impl Value {
    pub fn is_mac_address(&self) -> bool {
        matches!(self, Self::MacAddress(_))
    }

    /// The payload of a non-NULL [`Value::MacAddress`]; `None` otherwise.
    pub fn as_ref_mac_address(&self) -> Option<&MacAddress> {
        match self {
            Self::MacAddress(v) => v.as_deref(),
            _ => None,
        }
    }
}

impl IntoIterator for ValueTuple {
    type Item = Value;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        match self {
            ValueTuple::One(v) => vec![v].into_iter(),
            ValueTuple::Two(v, w) => vec![v, w].into_iter(),
            ValueTuple::Three(u, v, w) => vec![u, v, w].into_iter(),
            ValueTuple::Many(vec) => vec.into_iter(),
        }
    }
}

impl IntoValueTuple for ValueTuple {
    fn into_value_tuple(self) -> ValueTuple {
        self
    }
}

impl<V> IntoValueTuple for V
where
    V: Into<Value>,
{
    fn into_value_tuple(self) -> ValueTuple {
        ValueTuple::One(self.into())
    }
}

impl<V, W> IntoValueTuple for (V, W)
where
    V: Into<Value>,
    W: Into<Value>,
{
    fn into_value_tuple(self) -> ValueTuple {
        ValueTuple::Two(self.0.into(), self.1.into())
    }
}

impl<U, V, W> IntoValueTuple for (U, V, W)
where
    U: Into<Value>,
    V: Into<Value>,
    W: Into<Value>,
{
    fn into_value_tuple(self) -> ValueTuple {
        ValueTuple::Three(self.0.into(), self.1.into(), self.2.into())
    }
}

macro_rules! impl_into_value_tuple {
    ( $($idx:tt : $T:ident),+ $(,)? ) => {
        impl< $($T),+ > IntoValueTuple for ( $($T),+ )
        where
            $($T: Into<Value>),+
        {
            fn into_value_tuple(self) -> ValueTuple {
                ValueTuple::Many(vec![
                    $(self.$idx.into()),+
                ])
            }
        }
    };
}

#[rustfmt::skip]
mod impl_into_value_tuple {
    use super::*;

    impl_into_value_tuple!(0:T0, 1:T1, 2:T2, 3:T3);
    impl_into_value_tuple!(0:T0, 1:T1, 2:T2, 3:T3, 4:T4);
    impl_into_value_tuple!(0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5);
    impl_into_value_tuple!(0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5, 6:T6);
    impl_into_value_tuple!(0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5, 6:T6, 7:T7);
    impl_into_value_tuple!(0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5, 6:T6, 7:T7, 8:T8);
    impl_into_value_tuple!(0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5, 6:T6, 7:T7, 8:T8, 9:T9);
    impl_into_value_tuple!(0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5, 6:T6, 7:T7, 8:T8, 9:T9, 10:T10);
    impl_into_value_tuple!(0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5, 6:T6, 7:T7, 8:T8, 9:T9, 10:T10, 11:T11);
}

// [spec:pgorm:def:sql.value.tuple+2]
fn tuple_element<T>(value: Value, position: usize) -> Result<T, ValueTupleError>
where
    T: ValueType,
{
    <T as ValueType>::try_from(value).map_err(|_| ValueTupleError::Element {
        position,
        expected: T::type_name(),
    })
}

// [spec:pgorm:def:sql.value.tuple+2]
fn take_tuple_element<T>(
    iter: &mut std::vec::IntoIter<Value>,
    position: usize,
    expected: usize,
) -> Result<T, ValueTupleError>
where
    T: ValueType,
{
    match iter.next() {
        Some(value) => tuple_element(value, position),
        None => Err(ValueTupleError::Arity {
            expected: ValueTupleShape::Many(expected),
            actual: ValueTupleShape::Many(position),
        }),
    }
}

impl<V> TryFromValueTuple for V
where
    V: Into<Value> + ValueType,
{
    // [spec:pgorm:def:sql.value.tuple+2]
    fn try_from_value_tuple<I>(i: I) -> Result<Self, ValueTupleError>
    where
        I: IntoValueTuple,
    {
        match i.into_value_tuple() {
            ValueTuple::One(u) => tuple_element(u, 0),
            other => Err(ValueTupleError::Arity {
                expected: ValueTupleShape::One,
                actual: other.shape(),
            }),
        }
    }
}

impl<V, W> TryFromValueTuple for (V, W)
where
    V: Into<Value> + ValueType,
    W: Into<Value> + ValueType,
{
    // [spec:pgorm:def:sql.value.tuple+2]
    fn try_from_value_tuple<I>(i: I) -> Result<Self, ValueTupleError>
    where
        I: IntoValueTuple,
    {
        match i.into_value_tuple() {
            ValueTuple::Two(v, w) => Ok((tuple_element(v, 0)?, tuple_element(w, 1)?)),
            other => Err(ValueTupleError::Arity {
                expected: ValueTupleShape::Two,
                actual: other.shape(),
            }),
        }
    }
}

impl<U, V, W> TryFromValueTuple for (U, V, W)
where
    U: Into<Value> + ValueType,
    V: Into<Value> + ValueType,
    W: Into<Value> + ValueType,
{
    // [spec:pgorm:def:sql.value.tuple+2]
    fn try_from_value_tuple<I>(i: I) -> Result<Self, ValueTupleError>
    where
        I: IntoValueTuple,
    {
        match i.into_value_tuple() {
            ValueTuple::Three(u, v, w) => Ok((
                tuple_element(u, 0)?,
                tuple_element(v, 1)?,
                tuple_element(w, 2)?,
            )),
            other => Err(ValueTupleError::Arity {
                expected: ValueTupleShape::Three,
                actual: other.shape(),
            }),
        }
    }
}

macro_rules! impl_try_from_value_tuple {
    ( $len:expr, $($idx:tt : $T:ident),+ $(,)? ) => {
        impl< $($T),+ > TryFromValueTuple for ( $($T),+ )
        where
            $($T: Into<Value> + ValueType),+
        {
            // [spec:pgorm:def:sql.value.tuple+2]
            fn try_from_value_tuple<Z>(i: Z) -> Result<Self, ValueTupleError>
            where
                Z: IntoValueTuple,
            {
                let tuple = i.into_value_tuple();
                let actual = tuple.shape();
                match tuple {
                    ValueTuple::Many(vec) if vec.len() == $len => {
                        let mut iter = vec.into_iter();
                        Ok((
                            $(take_tuple_element::<$T>(&mut iter, $idx, $len)?),+
                        ))
                    }
                    _ => Err(ValueTupleError::Arity {
                        expected: ValueTupleShape::Many($len),
                        actual,
                    }),
                }
            }
        }
    };
}

#[rustfmt::skip]
mod impl_try_from_value_tuple {
    use super::*;

    impl_try_from_value_tuple!( 4, 0:T0, 1:T1, 2:T2, 3:T3);
    impl_try_from_value_tuple!( 5, 0:T0, 1:T1, 2:T2, 3:T3, 4:T4);
    impl_try_from_value_tuple!( 6, 0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5);
    impl_try_from_value_tuple!( 7, 0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5, 6:T6);
    impl_try_from_value_tuple!( 8, 0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5, 6:T6, 7:T7);
    impl_try_from_value_tuple!( 9, 0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5, 6:T6, 7:T7, 8:T8);
    impl_try_from_value_tuple!(10, 0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5, 6:T6, 7:T7, 8:T8, 9:T9);
    impl_try_from_value_tuple!(11, 0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5, 6:T6, 7:T7, 8:T8, 9:T9, 10:T10);
    impl_try_from_value_tuple!(12, 0:T0, 1:T1, 2:T2, 3:T3, 4:T4, 5:T5, 6:T6, 7:T7, 8:T8, 9:T9, 10:T10, 11:T11);
}

impl Values {
    pub fn iter(&self) -> impl Iterator<Item = &Value> {
        self.0.iter()
    }
}

impl IntoIterator for Values {
    type Item = Value;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    // [spec:pgorm:def:sql.value.conversions+1/test]
    #[test]
    fn test_value() {
        macro_rules! test_value {
            ( $type: ty, $val: literal ) => {
                let val: $type = $val;
                let v: Value = val.into();
                let out: $type = <$type as ValueType>::try_from(v).unwrap();
                assert_eq!(out, val);
            };
        }

        test_value!(u32, 4294967295);
        test_value!(i8, 127);
        test_value!(i16, 32767);
        test_value!(i32, 1073741824);
        test_value!(i64, 8589934592);
    }

    // [spec:pgorm:def:sql.value.value-type+3/test]
    #[test]
    fn test_option_value() {
        macro_rules! test_some_value {
            ( $type: ty, $val: literal ) => {
                let val: Option<$type> = Some($val);
                let v: Value = val.into();
                let out: $type = <$type as ValueType>::try_from(v).unwrap();
                assert_eq!(out, val.unwrap());
            };
        }

        macro_rules! test_none {
            ( $type: ty, $name: ident ) => {
                let val: Option<$type> = None;
                let v: Value = val.into();
                assert_eq!(v, Value::$name(None));
            };
        }

        test_some_value!(u32, 4294967295);
        test_some_value!(i8, 127);
        test_some_value!(i16, 32767);
        test_some_value!(i32, 1073741824);
        test_some_value!(i64, 8589934592);

        test_none!(u32, Unsigned);
        test_none!(i8, TinyInt);
        test_none!(i16, SmallInt);
        test_none!(i32, Int);
        test_none!(i64, BigInt);
    }

    #[test]
    fn test_cow_value() {
        let val: Cow<str> = "hello".into();
        let val2 = val.clone();
        let v: Value = val.into();
        let out: Cow<str> = <Cow<str> as ValueType>::try_from(v).unwrap();
        assert_eq!(out, val2);
    }

    #[test]
    fn test_box_value() {
        let val: String = "hello".to_owned();
        let v: Value = val.clone().into();
        let out: String = <String as ValueType>::try_from(v).unwrap();
        assert_eq!(out, val);
    }

    // [spec:pgorm:def:sql.value.tuple+2/test]
    #[test]
    fn test_value_tuple() {
        assert_eq!(
            1i32.into_value_tuple(),
            ValueTuple::One(Value::Int(Some(1)))
        );
        assert_eq!(
            "b".into_value_tuple(),
            ValueTuple::One(Value::String(Some(Box::new("b".to_owned()))))
        );
        assert_eq!(
            (1i32, "b").into_value_tuple(),
            ValueTuple::Two(
                Value::Int(Some(1)),
                Value::String(Some(Box::new("b".to_owned())))
            )
        );
        assert_eq!(
            (1i32, 2.4f64, "b").into_value_tuple(),
            ValueTuple::Three(
                Value::Int(Some(1)),
                Value::Double(Some(2.4)),
                Value::String(Some(Box::new("b".to_owned())))
            )
        );
        assert_eq!(
            (1i32, 2.4f64, "b", 123i8).into_value_tuple(),
            ValueTuple::Many(vec![
                Value::Int(Some(1)),
                Value::Double(Some(2.4)),
                Value::String(Some(Box::new("b".to_owned()))),
                Value::TinyInt(Some(123))
            ])
        );
        assert_eq!(
            (1i32, 2.4f64, "b", 123i8, 456i16).into_value_tuple(),
            ValueTuple::Many(vec![
                Value::Int(Some(1)),
                Value::Double(Some(2.4)),
                Value::String(Some(Box::new("b".to_owned()))),
                Value::TinyInt(Some(123)),
                Value::SmallInt(Some(456))
            ])
        );
        assert_eq!(
            (1i32, 2.4f64, "b", 123i8, 456i16, 789u32).into_value_tuple(),
            ValueTuple::Many(vec![
                Value::Int(Some(1)),
                Value::Double(Some(2.4)),
                Value::String(Some(Box::new("b".to_owned()))),
                Value::TinyInt(Some(123)),
                Value::SmallInt(Some(456)),
                Value::Unsigned(Some(789))
            ])
        );
    }

    // [spec:pgorm:def:sql.value.tuple+2/test]
    #[test]
    #[allow(clippy::clone_on_copy)]
    fn test_try_from_value_tuple() {
        let mut val = 1i32;
        let original = val.clone();
        val = TryFromValueTuple::try_from_value_tuple(val).unwrap();
        assert_eq!(val, original);

        let mut val = "b".to_owned();
        let original = val.clone();
        val = TryFromValueTuple::try_from_value_tuple(val).unwrap();
        assert_eq!(val, original);

        let mut val = (1i32, "b".to_owned());
        let original = val.clone();
        val = TryFromValueTuple::try_from_value_tuple(val).unwrap();
        assert_eq!(val, original);

        let mut val = (1i32, 2.4f64, "b".to_owned());
        let original = val.clone();
        val = TryFromValueTuple::try_from_value_tuple(val).unwrap();
        assert_eq!(val, original);

        let mut val = (1i32, 2.4f64, "b".to_owned(), 123i8);
        let original = val.clone();
        val = TryFromValueTuple::try_from_value_tuple(val).unwrap();
        assert_eq!(val, original);

        let mut val = (1i32, 2.4f64, "b".to_owned(), 123i8, 456i16);
        let original = val.clone();
        val = TryFromValueTuple::try_from_value_tuple(val).unwrap();
        assert_eq!(val, original);

        let mut val = (1i32, 2.4f64, "b".to_owned(), 123i8, 456i16, 789u32);
        let original = val.clone();
        val = TryFromValueTuple::try_from_value_tuple(val).unwrap();
        assert_eq!(val, original);
    }

    // [spec:pgorm:def:sql.value.tuple+2/test]
    #[test]
    fn value_tuple_shape_names_each_variant() {
        assert_eq!(ValueTuple::One(1i32.into()).shape(), ValueTupleShape::One);
        assert_eq!(
            ValueTuple::Two(1i32.into(), 2i32.into()).shape(),
            ValueTupleShape::Two
        );
        assert_eq!(
            ValueTuple::Three(1i32.into(), 2i32.into(), 3i32.into()).shape(),
            ValueTupleShape::Three
        );
        assert_eq!(
            ValueTuple::Many(vec![1i32.into(), 2i32.into()]).shape(),
            ValueTupleShape::Many(2)
        );
        assert_eq!(ValueTupleShape::One.to_string(), "ValueTuple::One");
        assert_eq!(ValueTupleShape::Two.to_string(), "ValueTuple::Two");
        assert_eq!(ValueTupleShape::Three.to_string(), "ValueTuple::Three");
        assert_eq!(
            ValueTupleShape::Many(5).to_string(),
            "ValueTuple::Many with length of 5"
        );
    }

    // [spec:pgorm:def:sql.value.tuple+2/test]
    #[test]
    fn try_from_value_tuple_errs_on_wrong_arity() {
        assert_eq!(
            <i32 as TryFromValueTuple>::try_from_value_tuple((1i32, 2i32)),
            Err(ValueTupleError::Arity {
                expected: ValueTupleShape::One,
                actual: ValueTupleShape::Two,
            })
        );
        assert_eq!(
            <(i32, i32) as TryFromValueTuple>::try_from_value_tuple(1i32),
            Err(ValueTupleError::Arity {
                expected: ValueTupleShape::Two,
                actual: ValueTupleShape::One,
            })
        );
        assert_eq!(
            <(i32, i32, i32) as TryFromValueTuple>::try_from_value_tuple((1i32, 2i32)),
            Err(ValueTupleError::Arity {
                expected: ValueTupleShape::Three,
                actual: ValueTupleShape::Two,
            })
        );
        assert_eq!(
            <(i32, i32, i32, i32) as TryFromValueTuple>::try_from_value_tuple((1i32, 2i32, 3i32)),
            Err(ValueTupleError::Arity {
                expected: ValueTupleShape::Many(4),
                actual: ValueTupleShape::Three,
            })
        );
        assert_eq!(
            <(i32, i32, i32, i32) as TryFromValueTuple>::try_from_value_tuple((
                1i32, 2i32, 3i32, 4i32, 5i32
            )),
            Err(ValueTupleError::Arity {
                expected: ValueTupleShape::Many(4),
                actual: ValueTupleShape::Many(5),
            })
        );
    }

    // [spec:pgorm:def:sql.value.tuple+2/test]
    #[test]
    fn try_from_value_tuple_errs_on_wrong_element() {
        assert_eq!(
            <String as TryFromValueTuple>::try_from_value_tuple(1i32),
            Err(ValueTupleError::Element {
                position: 0,
                expected: "String".to_owned(),
            })
        );
        assert_eq!(
            <(i32, String) as TryFromValueTuple>::try_from_value_tuple((1i32, 2i32)),
            Err(ValueTupleError::Element {
                position: 1,
                expected: "String".to_owned(),
            })
        );
        assert_eq!(
            <(i32, i32, i32, String) as TryFromValueTuple>::try_from_value_tuple((
                1i32, 2i32, 3i32, 4i32
            )),
            Err(ValueTupleError::Element {
                position: 3,
                expected: "String".to_owned(),
            })
        );
        assert_eq!(
            ValueTupleError::Arity {
                expected: ValueTupleShape::One,
                actual: ValueTupleShape::Many(3),
            }
            .to_string(),
            "expected ValueTuple::One, received ValueTuple::Many with length of 3"
        );
        assert_eq!(
            ValueTupleError::Element {
                position: 1,
                expected: "String".to_owned(),
            }
            .to_string(),
            "value at position 1 is not a valid `String`"
        );
    }

    #[test]
    fn test_value_tuple_iter() {
        let mut iter = (1i32).into_value_tuple().into_iter();
        assert_eq!(iter.next().unwrap(), Value::Int(Some(1)));
        assert_eq!(iter.next(), None);

        let mut iter = (1i32, 2.4f64).into_value_tuple().into_iter();
        assert_eq!(iter.next().unwrap(), Value::Int(Some(1)));
        assert_eq!(iter.next().unwrap(), Value::Double(Some(2.4)));
        assert_eq!(iter.next(), None);

        let mut iter = (1i32, 2.4f64, "b").into_value_tuple().into_iter();
        assert_eq!(iter.next().unwrap(), Value::Int(Some(1)));
        assert_eq!(iter.next().unwrap(), Value::Double(Some(2.4)));
        assert_eq!(
            iter.next().unwrap(),
            Value::String(Some(Box::new("b".to_owned())))
        );
        assert_eq!(iter.next(), None);

        let mut iter = (1i32, 2.4f64, "b", 123i8).into_value_tuple().into_iter();
        assert_eq!(iter.next().unwrap(), Value::Int(Some(1)));
        assert_eq!(iter.next().unwrap(), Value::Double(Some(2.4)));
        assert_eq!(
            iter.next().unwrap(),
            Value::String(Some(Box::new("b".to_owned())))
        );
        assert_eq!(iter.next().unwrap(), Value::TinyInt(Some(123)));
        assert_eq!(iter.next(), None);

        let mut iter = (1i32, 2.4f64, "b", 123i8, 456i16)
            .into_value_tuple()
            .into_iter();
        assert_eq!(iter.next().unwrap(), Value::Int(Some(1)));
        assert_eq!(iter.next().unwrap(), Value::Double(Some(2.4)));
        assert_eq!(
            iter.next().unwrap(),
            Value::String(Some(Box::new("b".to_owned())))
        );
        assert_eq!(iter.next().unwrap(), Value::TinyInt(Some(123)));
        assert_eq!(iter.next().unwrap(), Value::SmallInt(Some(456)));
        assert_eq!(iter.next(), None);

        let mut iter = (1i32, 2.4f64, "b", 123i8, 456i16, 789u32)
            .into_value_tuple()
            .into_iter();
        assert_eq!(iter.next().unwrap(), Value::Int(Some(1)));
        assert_eq!(iter.next().unwrap(), Value::Double(Some(2.4)));
        assert_eq!(
            iter.next().unwrap(),
            Value::String(Some(Box::new("b".to_owned())))
        );
        assert_eq!(iter.next().unwrap(), Value::TinyInt(Some(123)));
        assert_eq!(iter.next().unwrap(), Value::SmallInt(Some(456)));
        assert_eq!(iter.next().unwrap(), Value::Unsigned(Some(789)));
        assert_eq!(iter.next(), None);
    }

    #[test]

    fn test_json_value() {
        let json = serde_json::json! {{
            "a": 25.0,
            "b": "hello",
        }};
        let value: Value = json.clone().into();
        let out: Json = <Json as ValueType>::try_from(value).unwrap();
        assert_eq!(out, json);
    }

    #[test]

    fn test_chrono_value() {
        let timestamp = NaiveDate::from_ymd_opt(2020, 1, 1)
            .unwrap()
            .and_hms_opt(2, 2, 2)
            .unwrap();
        let value: Value = timestamp.into();
        let out: NaiveDateTime = <NaiveDateTime as ValueType>::try_from(value).unwrap();
        assert_eq!(out, timestamp);
    }

    #[test]

    fn test_chrono_utc_value() {
        let timestamp = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDate::from_ymd_opt(2022, 1, 2)
                .unwrap()
                .and_hms_opt(3, 4, 5)
                .unwrap(),
            Utc,
        );
        let value: Value = timestamp.into();
        let out: DateTime<Utc> = <DateTime<Utc> as ValueType>::try_from(value).unwrap();
        assert_eq!(out, timestamp);
    }

    #[test]

    fn test_chrono_local_value() {
        let timestamp_utc = DateTime::<Utc>::from_naive_utc_and_offset(
            NaiveDate::from_ymd_opt(2022, 1, 2)
                .unwrap()
                .and_hms_opt(3, 4, 5)
                .unwrap(),
            Utc,
        );
        let timestamp_local: DateTime<Local> = timestamp_utc.into();
        let value: Value = timestamp_local.into();
        let out: DateTime<Local> = <DateTime<Local> as ValueType>::try_from(value).unwrap();
        assert_eq!(out, timestamp_local);
    }

    #[test]

    fn test_chrono_timezone_value() {
        let timestamp = DateTime::parse_from_rfc3339("2020-01-01T02:02:02+08:00").unwrap();
        let value: Value = timestamp.into();
        let out: DateTime<FixedOffset> =
            <DateTime<FixedOffset> as ValueType>::try_from(value).unwrap();
        assert_eq!(out, timestamp);
    }

    // [spec:pgorm:sem:sql.value.render/test]
    #[test]

    fn test_chrono_query() {
        use crate::*;

        let string = "2020-01-01T02:02:02+08:00";
        let timestamp = DateTime::parse_from_rfc3339(string).unwrap();

        let query = Query::select().expr(timestamp).to_owned();

        let formatted = "2020-01-01 02:02:02 +08:00";

        assert_eq!(query.to_string(), format!("SELECT '{formatted}'"));
    }

    // [spec:pgorm:def:sql.value.conversions+1/test]
    #[test]
    fn test_uuid_value() {
        let uuid = Uuid::parse_str("936DA01F9ABD4d9d80C702AF85C822A8").unwrap();
        let value: Value = uuid.into();
        let out: Uuid = <Uuid as ValueType>::try_from(value).unwrap();
        assert_eq!(out, uuid);

        let uuid_braced = uuid.braced();
        let value: Value = uuid_braced.into();
        let out: Uuid = <Uuid as ValueType>::try_from(value).unwrap();
        assert_eq!(out, uuid);

        let uuid_hyphenated = uuid.hyphenated();
        let value: Value = uuid_hyphenated.into();
        let out: Uuid = <Uuid as ValueType>::try_from(value).unwrap();
        assert_eq!(out, uuid);

        let uuid_simple = uuid.simple();
        let value: Value = uuid_simple.into();
        let out: Uuid = <Uuid as ValueType>::try_from(value).unwrap();
        assert_eq!(out, uuid);

        let uuid_urn = uuid.urn();
        let value: Value = uuid_urn.into();
        let out: Uuid = <Uuid as ValueType>::try_from(value).unwrap();
        assert_eq!(out, uuid);
    }

    #[test]

    fn test_decimal_value() {
        use std::str::FromStr;

        let num = "2.02";
        let val = Decimal::from_str(num).unwrap();
        let v: Value = val.into();
        let out: Decimal = <Decimal as ValueType>::try_from(v).unwrap();
        assert_eq!(out.to_string(), num);
    }

    // [spec:pgorm:def:sql.value.array+4/test]
    #[test]
    fn test_array_value() {
        let array = vec![1, 2, 3, 4, 5];
        let v: Value = array.into();
        let out: Vec<i32> = <Vec<i32> as ValueType>::try_from(v).unwrap();
        assert_eq!(out, vec![1, 2, 3, 4, 5]);
    }

    // [spec:pgorm:def:sql.value.array+4/test]
    #[test]
    fn test_option_array_value() {
        let v: Value = Value::Array(ArrayType::Int, None);
        let out: Option<Vec<i32>> = <Option<Vec<i32>> as ValueType>::try_from(v).unwrap();
        assert_eq!(out, None);
    }

    // [spec:pgorm:def:sql.value.array+4/test]
    #[test]
    fn vector_has_an_array_type_tag() {
        assert_eq!(<Vector as ValueType>::array_type(), ArrayType::Vector);
        assert_eq!(ArrayType::Vector.source_type_name(), None);
    }
}
