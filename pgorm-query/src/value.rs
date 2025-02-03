//! Container for all SQL value types.

use std::{borrow::Cow, hash::Hash};

use serde_json::Value as Json;
use std::str::from_utf8;

use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};

use rust_decimal::Decimal;

use uuid::Uuid;

use ipnetwork::IpNetwork;

use std::net::IpAddr;

use mac_address::MacAddress;

use crate::{ColumnType, QueryBuilder, StringLen};

/// [`Value`] types variant for Postgres array
#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub enum ArrayType {
    Bool,
    TinyInt,
    SmallInt,
    Int,
    BigInt,
    TinyUnsigned,
    SmallUnsigned,
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
}

/// Value variants
///
/// We want the inner Value to be exactly 1 pointer sized, so anything larger should be boxed.
///
/// If the `hashable-value` feature is enabled, NaN == NaN, which contradicts Rust's built-in
/// implementation of NaN != NaN.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Bool(Option<bool>),
    TinyInt(Option<i8>),
    SmallInt(Option<i16>),
    Int(Option<i32>),
    BigInt(Option<i64>),
    TinyUnsigned(Option<u8>),
    SmallUnsigned(Option<u16>),
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

impl Eq for Value {}

fn hash_f32<H: std::hash::Hasher>(value: Option<f32>, state: &mut H) {
    match value {
        Some(value) => value.to_bits().hash(state),
        None => state.write_u8(0),
    }
}

fn hash_f64<H: std::hash::Hasher>(value: Option<f64>, state: &mut H) {
    match value {
        Some(value) => value.to_bits().hash(state),
        None => state.write_u8(0),
    }
}

fn hash_vector<H: std::hash::Hasher>(value: Option<&pgvector::Vector>, state: &mut H) {
    match value {
        Some(value) => {
            let slice = value.as_slice();
            unsafe { std::mem::transmute::<&[f32], &[u32]>(slice) }.hash(state);
        }
        None => state.write_u8(0),
    }
}

impl Hash for Value {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        match self {
            Value::Bool(value) => value.hash(state),
            Value::TinyInt(value) => value.hash(state),
            Value::SmallInt(value) => value.hash(state),
            Value::Int(value) => value.hash(state),
            Value::BigInt(value) => value.hash(state),
            Value::TinyUnsigned(value) => value.hash(state),
            Value::SmallUnsigned(value) => value.hash(state),
            Value::Unsigned(value) => value.hash(state),
            Value::BigUnsigned(value) => value.hash(state),
            Value::Float(value) => hash_f32(*value, state),
            Value::Double(value) => hash_f64(*value, state),
            Value::String(value) => value.hash(state),
            Value::Char(value) => value.hash(state),
            Value::Bytes(value) => value.hash(state),
            Value::Json(value) => value.hash(state),
            Value::ChronoDate(value) => value.hash(state),
            Value::ChronoTime(value) => value.hash(state),
            Value::ChronoDateTime(value) => value.hash(state),
            Value::ChronoDateTimeUtc(value) => value.hash(state),
            Value::ChronoDateTimeLocal(value) => value.hash(state),
            Value::ChronoDateTimeWithTimeZone(value) => value.hash(state),
            Value::Uuid(value) => value.hash(state),
            Value::Decimal(value) => value.hash(state),
            Value::Array(_, value) => value.hash(state),
            Value::Vector(value) => hash_vector(value.as_deref(), state),
            Value::IpNetwork(value) => value.hash(state),
            Value::MacAddress(value) => value.hash(state),
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", QueryBuilder.value_to_string(self))
    }
}

pub trait ValueType: Sized {
    fn try_from(v: Value) -> Result<Self, ValueTypeErr>;

    fn unwrap(v: Value) -> Self {
        Self::try_from(v).unwrap()
    }

    fn expect(v: Value, msg: &str) -> Self {
        Self::try_from(v).expect(msg)
    }

    fn type_name() -> String;

    fn array_type() -> ArrayType;

    fn column_type() -> ColumnType;
}

#[derive(Debug)]
pub struct ValueTypeErr;

impl std::error::Error for ValueTypeErr {}

impl std::fmt::Display for ValueTypeErr {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "Value type mismatch")
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Values(pub Vec<Value>);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum ValueTuple {
    One(Value),
    Two(Value, Value),
    Three(Value, Value, Value),
    Many(Vec<Value>),
}

pub trait IntoValueTuple {
    fn into_value_tuple(self) -> ValueTuple;
}

pub trait FromValueTuple: Sized {
    fn from_value_tuple<I>(i: I) -> Self
    where
        I: IntoValueTuple;
}

pub trait Nullable {
    fn null() -> Value;
}

impl Value {
    pub fn unwrap<T>(self) -> T
    where
        T: ValueType,
    {
        T::unwrap(self)
    }

    pub fn expect<T>(self, msg: &str) -> T
    where
        T: ValueType,
    {
        T::expect(self, msg)
    }
}

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
            fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
                match v {
                    Value::$name(Some(x)) => Ok(x),
                    _ => Err(ValueTypeErr),
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
            fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
                match v {
                    Value::$name(Some(x)) => Ok(*x),
                    _ => Err(ValueTypeErr),
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
type_to_value!(i8, TinyInt, TinyInteger);
type_to_value!(i16, SmallInt, SmallInteger);
type_to_value!(i32, Int, Integer);
type_to_value!(i64, BigInt, BigInteger);
type_to_value!(u8, TinyUnsigned, TinyUnsigned);
type_to_value!(u16, SmallUnsigned, SmallUnsigned);
type_to_value!(u32, Unsigned, Unsigned);
type_to_value!(u64, BigUnsigned, BigUnsigned);
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

impl<T> ValueType for Option<T>
where
    T: ValueType + Nullable,
{
    fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
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
    fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
        match v {
            Value::String(Some(x)) => Ok((*x).into()),
            _ => Err(ValueTypeErr),
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

type_to_box_value!(Vec<u8>, Bytes, VarBinary(StringLen::None));
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
    type_to_box_value!(NaiveDateTime, ChronoDateTime, DateTime);

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
        fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
            match v {
                Value::ChronoDateTimeUtc(Some(x)) => Ok(*x),
                _ => Err(ValueTypeErr),
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
        fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
            match v {
                Value::ChronoDateTimeLocal(Some(x)) => Ok(*x),
                _ => Err(ValueTypeErr),
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
        fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
            match v {
                Value::ChronoDateTimeWithTimeZone(Some(x)) => Ok(*x),
                _ => Err(ValueTypeErr),
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
                fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
                    match v {
                        Value::Uuid(Some(x)) => Ok(x.$conversion_fn()),
                        _ => Err(ValueTypeErr),
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

pub mod with_array {
    use super::*;
    use crate::RcOrArc;

    // We only imlement conversion from Vec<T> to Array when T is not u8.
    // This is because for u8's case, there is already conversion to Byte defined above.
    // TODO When negative trait becomes a stable feature, following code can be much shorter.
    pub trait NotU8 {}

    impl NotU8 for bool {}
    impl NotU8 for i8 {}
    impl NotU8 for i16 {}
    impl NotU8 for i32 {}
    impl NotU8 for i64 {}
    impl NotU8 for u16 {}
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
        fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
            match v {
                Value::Array(ty, Some(v)) if T::array_type() == ty => {
                    Ok(v.into_iter().map(|e| e.unwrap()).collect())
                }
                _ => Err(ValueTypeErr),
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
            Array(RcOrArc::new(T::column_type()))
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
        fn try_from(v: Value) -> Result<Self, ValueTypeErr> {
            match v {
                Value::Vector(Some(x)) => Ok(*x),
                _ => Err(ValueTypeErr),
            }
        }

        fn type_name() -> String {
            stringify!(Vector).to_owned()
        }

        fn array_type() -> ArrayType {
            unimplemented!("Vector does not have array type")
        }

        fn column_type() -> ColumnType {
            ColumnType::Vector(None)
        }
    }
}

#[allow(unused_macros)]
macro_rules! box_to_opt_ref {
    ( $v: expr ) => {
        match $v {
            Some(v) => Some(v.as_ref()),
            None => None,
        }
    };
}

impl Value {
    pub fn is_json(&self) -> bool {
        matches!(self, Self::Json(_))
    }

    pub fn as_ref_json(&self) -> Option<&Json> {
        match self {
            Self::Json(v) => box_to_opt_ref!(v),
            _ => panic!("not Value::Json"),
        }
    }
}

impl Value {
    pub fn is_chrono_date(&self) -> bool {
        matches!(self, Self::ChronoDate(_))
    }

    pub fn as_ref_chrono_date(&self) -> Option<&NaiveDate> {
        match self {
            Self::ChronoDate(v) => box_to_opt_ref!(v),
            _ => panic!("not Value::ChronoDate"),
        }
    }
}

impl Value {
    pub fn is_chrono_time(&self) -> bool {
        matches!(self, Self::ChronoTime(_))
    }

    pub fn as_ref_chrono_time(&self) -> Option<&NaiveTime> {
        match self {
            Self::ChronoTime(v) => box_to_opt_ref!(v),
            _ => panic!("not Value::ChronoTime"),
        }
    }
}

impl Value {
    pub fn is_chrono_date_time(&self) -> bool {
        matches!(self, Self::ChronoDateTime(_))
    }

    pub fn as_ref_chrono_date_time(&self) -> Option<&NaiveDateTime> {
        match self {
            Self::ChronoDateTime(v) => box_to_opt_ref!(v),
            _ => panic!("not Value::ChronoDateTime"),
        }
    }
}

impl Value {
    pub fn is_chrono_date_time_utc(&self) -> bool {
        matches!(self, Self::ChronoDateTimeUtc(_))
    }

    pub fn as_ref_chrono_date_time_utc(&self) -> Option<&DateTime<Utc>> {
        match self {
            Self::ChronoDateTimeUtc(v) => box_to_opt_ref!(v),
            _ => panic!("not Value::ChronoDateTimeUtc"),
        }
    }
}

impl Value {
    pub fn is_chrono_date_time_local(&self) -> bool {
        matches!(self, Self::ChronoDateTimeLocal(_))
    }

    pub fn as_ref_chrono_date_time_local(&self) -> Option<&DateTime<Local>> {
        match self {
            Self::ChronoDateTimeLocal(v) => box_to_opt_ref!(v),
            _ => panic!("not Value::ChronoDateTimeLocal"),
        }
    }
}

impl Value {
    pub fn is_chrono_date_time_with_time_zone(&self) -> bool {
        matches!(self, Self::ChronoDateTimeWithTimeZone(_))
    }

    pub fn as_ref_chrono_date_time_with_time_zone(&self) -> Option<&DateTime<FixedOffset>> {
        match self {
            Self::ChronoDateTimeWithTimeZone(v) => box_to_opt_ref!(v),
            _ => panic!("not Value::ChronoDateTimeWithTimeZone"),
        }
    }
}

impl Value {
    pub fn chrono_as_naive_utc_in_string(&self) -> Option<String> {
        match self {
            Self::ChronoDate(v) => v.as_ref().map(|v| v.to_string()),
            Self::ChronoTime(v) => v.as_ref().map(|v| v.to_string()),
            Self::ChronoDateTime(v) => v.as_ref().map(|v| v.to_string()),
            Self::ChronoDateTimeUtc(v) => v.as_ref().map(|v| v.naive_utc().to_string()),
            Self::ChronoDateTimeLocal(v) => v.as_ref().map(|v| v.naive_utc().to_string()),
            Self::ChronoDateTimeWithTimeZone(v) => v.as_ref().map(|v| v.naive_utc().to_string()),
            _ => panic!("not chrono Value"),
        }
    }
}

impl Value {
    pub fn is_decimal(&self) -> bool {
        matches!(self, Self::Decimal(_))
    }

    pub fn as_ref_decimal(&self) -> Option<&Decimal> {
        match self {
            Self::Decimal(v) => box_to_opt_ref!(v),
            _ => panic!("not Value::Decimal"),
        }
    }

    pub fn decimal_to_f64(&self) -> Option<f64> {
        use rust_decimal::prelude::ToPrimitive;

        self.as_ref_decimal().map(|d| d.to_f64().unwrap())
    }
}

impl Value {
    pub fn is_uuid(&self) -> bool {
        matches!(self, Self::Uuid(_))
    }
    pub fn as_ref_uuid(&self) -> Option<&Uuid> {
        match self {
            Self::Uuid(v) => box_to_opt_ref!(v),
            _ => panic!("not Value::Uuid"),
        }
    }
}

impl Value {
    pub fn is_array(&self) -> bool {
        matches!(self, Self::Array(_, _))
    }

    pub fn as_ref_array(&self) -> Option<&Vec<Value>> {
        match self {
            Self::Array(_, v) => box_to_opt_ref!(v),
            _ => panic!("not Value::Array"),
        }
    }
}

impl Value {
    pub fn is_ipnetwork(&self) -> bool {
        matches!(self, Self::IpNetwork(_))
    }

    pub fn as_ref_ipnetwork(&self) -> Option<&IpNetwork> {
        match self {
            Self::IpNetwork(v) => box_to_opt_ref!(v),
            _ => panic!("not Value::IpNetwork"),
        }
    }

    pub fn as_ipaddr(&self) -> Option<IpAddr> {
        match self {
            Self::IpNetwork(v) => v.clone().map(|v| v.network()),
            _ => panic!("not Value::IpNetwork"),
        }
    }
}

impl Value {
    pub fn is_mac_address(&self) -> bool {
        matches!(self, Self::MacAddress(_))
    }

    pub fn as_ref_mac_address(&self) -> Option<&MacAddress> {
        match self {
            Self::MacAddress(v) => box_to_opt_ref!(v),
            _ => panic!("not Value::MacAddress"),
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

impl<V> FromValueTuple for V
where
    V: Into<Value> + ValueType,
{
    fn from_value_tuple<I>(i: I) -> Self
    where
        I: IntoValueTuple,
    {
        match i.into_value_tuple() {
            ValueTuple::One(u) => u.unwrap(),
            _ => panic!("not ValueTuple::One"),
        }
    }
}

impl<V, W> FromValueTuple for (V, W)
where
    V: Into<Value> + ValueType,
    W: Into<Value> + ValueType,
{
    fn from_value_tuple<I>(i: I) -> Self
    where
        I: IntoValueTuple,
    {
        match i.into_value_tuple() {
            ValueTuple::Two(v, w) => (v.unwrap(), w.unwrap()),
            _ => panic!("not ValueTuple::Two"),
        }
    }
}

impl<U, V, W> FromValueTuple for (U, V, W)
where
    U: Into<Value> + ValueType,
    V: Into<Value> + ValueType,
    W: Into<Value> + ValueType,
{
    fn from_value_tuple<I>(i: I) -> Self
    where
        I: IntoValueTuple,
    {
        match i.into_value_tuple() {
            ValueTuple::Three(u, v, w) => (u.unwrap(), v.unwrap(), w.unwrap()),
            _ => panic!("not ValueTuple::Three"),
        }
    }
}

macro_rules! impl_from_value_tuple {
    ( $len:expr, $($T:ident),+ $(,)? ) => {
        impl< $($T),+ > FromValueTuple for ( $($T),+ )
        where
            $($T: Into<Value> + ValueType),+
        {
            fn from_value_tuple<Z>(i: Z) -> Self
            where
                Z: IntoValueTuple,
            {
                match i.into_value_tuple() {
                    ValueTuple::Many(vec) if vec.len() == $len => {
                        let mut iter = vec.into_iter();
                        (
                            $(<$T as ValueType>::unwrap(iter.next().unwrap())),+
                        )
                    }
                    _ => panic!("not ValueTuple::Many with length of {}", $len),
                }
            }
        }
    };
}

#[rustfmt::skip]
mod impl_from_value_tuple {
    use super::*;

    impl_from_value_tuple!( 4, T0, T1, T2, T3);
    impl_from_value_tuple!( 5, T0, T1, T2, T3, T4);
    impl_from_value_tuple!( 6, T0, T1, T2, T3, T4, T5);
    impl_from_value_tuple!( 7, T0, T1, T2, T3, T4, T5, T6);
    impl_from_value_tuple!( 8, T0, T1, T2, T3, T4, T5, T6, T7);
    impl_from_value_tuple!( 9, T0, T1, T2, T3, T4, T5, T6, T7, T8);
    impl_from_value_tuple!(10, T0, T1, T2, T3, T4, T5, T6, T7, T8, T9);
    impl_from_value_tuple!(11, T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10);
    impl_from_value_tuple!(12, T0, T1, T2, T3, T4, T5, T6, T7, T8, T9, T10, T11);
}

/// Convert value to json value
#[allow(clippy::many_single_char_names)]

pub fn sea_value_to_json_value(value: &Value) -> Json {
    match value {
        Value::Bool(None)
        | Value::TinyInt(None)
        | Value::SmallInt(None)
        | Value::Int(None)
        | Value::BigInt(None)
        | Value::TinyUnsigned(None)
        | Value::SmallUnsigned(None)
        | Value::Unsigned(None)
        | Value::BigUnsigned(None)
        | Value::Float(None)
        | Value::Double(None)
        | Value::String(None)
        | Value::Char(None)
        | Value::Bytes(None)
        | Value::Json(None) => Json::Null,

        Value::Decimal(None) => Json::Null,

        Value::Uuid(None) => Json::Null,
        Value::Array(_, None) => Json::Null,
        Value::Vector(None) => Json::Null,

        Value::IpNetwork(None) => Json::Null,

        Value::MacAddress(None) => Json::Null,
        Value::Bool(Some(b)) => Json::Bool(*b),
        Value::TinyInt(Some(v)) => (*v).into(),
        Value::SmallInt(Some(v)) => (*v).into(),
        Value::Int(Some(v)) => (*v).into(),
        Value::BigInt(Some(v)) => (*v).into(),
        Value::TinyUnsigned(Some(v)) => (*v).into(),
        Value::SmallUnsigned(Some(v)) => (*v).into(),
        Value::Unsigned(Some(v)) => (*v).into(),
        Value::BigUnsigned(Some(v)) => (*v).into(),
        Value::Float(Some(v)) => (*v).into(),
        Value::Double(Some(v)) => (*v).into(),
        Value::String(Some(s)) => Json::String(s.as_ref().clone()),
        Value::Char(Some(v)) => Json::String(v.to_string()),
        Value::Bytes(Some(s)) => Json::String(from_utf8(s).unwrap().to_string()),
        Value::Json(Some(v)) => v.as_ref().clone(),

        Value::ChronoDate(_) => QueryBuilder.value_to_string(value).into(),

        Value::ChronoTime(_) => QueryBuilder.value_to_string(value).into(),

        Value::ChronoDateTime(_) => QueryBuilder.value_to_string(value).into(),

        Value::ChronoDateTimeWithTimeZone(_) => QueryBuilder.value_to_string(value).into(),

        Value::ChronoDateTimeUtc(_) => QueryBuilder.value_to_string(value).into(),

        Value::ChronoDateTimeLocal(_) => QueryBuilder.value_to_string(value).into(),

        Value::Decimal(Some(v)) => {
            use rust_decimal::prelude::ToPrimitive;
            v.as_ref().to_f64().unwrap().into()
        }

        Value::Uuid(Some(v)) => Json::String(v.to_string()),
        Value::Array(_, Some(v)) => {
            Json::Array(v.as_ref().iter().map(sea_value_to_json_value).collect())
        }
        Value::Vector(Some(v)) => Json::Array(v.as_slice().iter().map(|&v| v.into()).collect()),

        Value::IpNetwork(Some(_)) => QueryBuilder.value_to_string(value).into(),

        Value::MacAddress(Some(_)) => QueryBuilder.value_to_string(value).into(),
    }
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

    #[test]
    fn test_value() {
        macro_rules! test_value {
            ( $type: ty, $val: literal ) => {
                let val: $type = $val;
                let v: Value = val.into();
                let out: $type = v.unwrap();
                assert_eq!(out, val);
            };
        }

        test_value!(u8, 255);
        test_value!(u16, 65535);
        test_value!(i8, 127);
        test_value!(i16, 32767);
        test_value!(i32, 1073741824);
        test_value!(i64, 8589934592);
    }

    #[test]
    fn test_option_value() {
        macro_rules! test_some_value {
            ( $type: ty, $val: literal ) => {
                let val: Option<$type> = Some($val);
                let v: Value = val.into();
                let out: $type = v.unwrap();
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

        test_some_value!(u8, 255);
        test_some_value!(u16, 65535);
        test_some_value!(i8, 127);
        test_some_value!(i16, 32767);
        test_some_value!(i32, 1073741824);
        test_some_value!(i64, 8589934592);

        test_none!(u8, TinyUnsigned);
        test_none!(u16, SmallUnsigned);
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
        let out: Cow<str> = v.unwrap();
        assert_eq!(out, val2);
    }

    #[test]
    fn test_box_value() {
        let val: String = "hello".to_owned();
        let v: Value = val.clone().into();
        let out: String = v.unwrap();
        assert_eq!(out, val);
    }

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
            (1i32, 2.4f64, "b", 123u8).into_value_tuple(),
            ValueTuple::Many(vec![
                Value::Int(Some(1)),
                Value::Double(Some(2.4)),
                Value::String(Some(Box::new("b".to_owned()))),
                Value::TinyUnsigned(Some(123))
            ])
        );
        assert_eq!(
            (1i32, 2.4f64, "b", 123u8, 456u16).into_value_tuple(),
            ValueTuple::Many(vec![
                Value::Int(Some(1)),
                Value::Double(Some(2.4)),
                Value::String(Some(Box::new("b".to_owned()))),
                Value::TinyUnsigned(Some(123)),
                Value::SmallUnsigned(Some(456))
            ])
        );
        assert_eq!(
            (1i32, 2.4f64, "b", 123u8, 456u16, 789u32).into_value_tuple(),
            ValueTuple::Many(vec![
                Value::Int(Some(1)),
                Value::Double(Some(2.4)),
                Value::String(Some(Box::new("b".to_owned()))),
                Value::TinyUnsigned(Some(123)),
                Value::SmallUnsigned(Some(456)),
                Value::Unsigned(Some(789))
            ])
        );
    }

    #[test]
    #[allow(clippy::clone_on_copy)]
    fn test_from_value_tuple() {
        let mut val = 1i32;
        let original = val.clone();
        val = FromValueTuple::from_value_tuple(val);
        assert_eq!(val, original);

        let mut val = "b".to_owned();
        let original = val.clone();
        val = FromValueTuple::from_value_tuple(val);
        assert_eq!(val, original);

        let mut val = (1i32, "b".to_owned());
        let original = val.clone();
        val = FromValueTuple::from_value_tuple(val);
        assert_eq!(val, original);

        let mut val = (1i32, 2.4f64, "b".to_owned());
        let original = val.clone();
        val = FromValueTuple::from_value_tuple(val);
        assert_eq!(val, original);

        let mut val = (1i32, 2.4f64, "b".to_owned(), 123u8);
        let original = val.clone();
        val = FromValueTuple::from_value_tuple(val);
        assert_eq!(val, original);

        let mut val = (1i32, 2.4f64, "b".to_owned(), 123u8, 456u16);
        let original = val.clone();
        val = FromValueTuple::from_value_tuple(val);
        assert_eq!(val, original);

        let mut val = (1i32, 2.4f64, "b".to_owned(), 123u8, 456u16, 789u32);
        let original = val.clone();
        val = FromValueTuple::from_value_tuple(val);
        assert_eq!(val, original);
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

        let mut iter = (1i32, 2.4f64, "b", 123u8).into_value_tuple().into_iter();
        assert_eq!(iter.next().unwrap(), Value::Int(Some(1)));
        assert_eq!(iter.next().unwrap(), Value::Double(Some(2.4)));
        assert_eq!(
            iter.next().unwrap(),
            Value::String(Some(Box::new("b".to_owned())))
        );
        assert_eq!(iter.next().unwrap(), Value::TinyUnsigned(Some(123)));
        assert_eq!(iter.next(), None);

        let mut iter = (1i32, 2.4f64, "b", 123u8, 456u16)
            .into_value_tuple()
            .into_iter();
        assert_eq!(iter.next().unwrap(), Value::Int(Some(1)));
        assert_eq!(iter.next().unwrap(), Value::Double(Some(2.4)));
        assert_eq!(
            iter.next().unwrap(),
            Value::String(Some(Box::new("b".to_owned())))
        );
        assert_eq!(iter.next().unwrap(), Value::TinyUnsigned(Some(123)));
        assert_eq!(iter.next().unwrap(), Value::SmallUnsigned(Some(456)));
        assert_eq!(iter.next(), None);

        let mut iter = (1i32, 2.4f64, "b", 123u8, 456u16, 789u32)
            .into_value_tuple()
            .into_iter();
        assert_eq!(iter.next().unwrap(), Value::Int(Some(1)));
        assert_eq!(iter.next().unwrap(), Value::Double(Some(2.4)));
        assert_eq!(
            iter.next().unwrap(),
            Value::String(Some(Box::new("b".to_owned())))
        );
        assert_eq!(iter.next().unwrap(), Value::TinyUnsigned(Some(123)));
        assert_eq!(iter.next().unwrap(), Value::SmallUnsigned(Some(456)));
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
        let out: Json = value.unwrap();
        assert_eq!(out, json);
    }

    #[test]

    fn test_chrono_value() {
        let timestamp = NaiveDate::from_ymd_opt(2020, 1, 1)
            .unwrap()
            .and_hms_opt(2, 2, 2)
            .unwrap();
        let value: Value = timestamp.into();
        let out: NaiveDateTime = value.unwrap();
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
        let out: DateTime<Utc> = value.unwrap();
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
        let out: DateTime<Local> = value.unwrap();
        assert_eq!(out, timestamp_local);
    }

    #[test]

    fn test_chrono_timezone_value() {
        let timestamp = DateTime::parse_from_rfc3339("2020-01-01T02:02:02+08:00").unwrap();
        let value: Value = timestamp.into();
        let out: DateTime<FixedOffset> = value.unwrap();
        assert_eq!(out, timestamp);
    }

    #[test]

    fn test_chrono_query() {
        use crate::*;

        let string = "2020-01-01T02:02:02+08:00";
        let timestamp = DateTime::parse_from_rfc3339(string).unwrap();

        let query = Query::select().expr(timestamp).to_owned();

        let formatted = "2020-01-01 02:02:02 +08:00";

        assert_eq!(
            query.to_string(QueryBuilder),
            format!("SELECT '{formatted}'")
        );
    }

    #[test]
    fn test_uuid_value() {
        let uuid = Uuid::parse_str("936DA01F9ABD4d9d80C702AF85C822A8").unwrap();
        let value: Value = uuid.into();
        let out: Uuid = value.unwrap();
        assert_eq!(out, uuid);

        let uuid_braced = uuid.braced();
        let value: Value = uuid_braced.into();
        let out: Uuid = value.unwrap();
        assert_eq!(out, uuid);

        let uuid_hyphenated = uuid.hyphenated();
        let value: Value = uuid_hyphenated.into();
        let out: Uuid = value.unwrap();
        assert_eq!(out, uuid);

        let uuid_simple = uuid.simple();
        let value: Value = uuid_simple.into();
        let out: Uuid = value.unwrap();
        assert_eq!(out, uuid);

        let uuid_urn = uuid.urn();
        let value: Value = uuid_urn.into();
        let out: Uuid = value.unwrap();
        assert_eq!(out, uuid);
    }

    #[test]

    fn test_decimal_value() {
        use std::str::FromStr;

        let num = "2.02";
        let val = Decimal::from_str(num).unwrap();
        let v: Value = val.into();
        let out: Decimal = v.unwrap();
        assert_eq!(out.to_string(), num);
    }

    #[test]
    fn test_array_value() {
        let array = vec![1, 2, 3, 4, 5];
        let v: Value = array.into();
        let out: Vec<i32> = v.unwrap();
        assert_eq!(out, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn test_option_array_value() {
        let v: Value = Value::Array(ArrayType::Int, None);
        let out: Option<Vec<i32>> = v.unwrap();
        assert_eq!(out, None);
    }
}
