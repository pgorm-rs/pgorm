//! One value of every variant of the open enums pgorm's bindings translate,
//! for the tests that hold each binding to every variant.
//!
//! An open enum is `#[non_exhaustive]` (`sql.surface`), so a binding's match
//! over one ends in a wildcard arm that a variant added later reaches without
//! breaking its build. A binding's test walks [`Variants::variants`] through
//! its translation and fails on any value that reaches that arm. The lists
//! live here, behind the hidden `tests-cfg` feature, rather than as public
//! constants: they are test fixtures, not a second spelling of each enum that
//! callers could come to depend on, and inside pgorm-query each list is held
//! to its enum by an exhaustive match, so it cannot fall behind.

use std::{mem::discriminant, ops::Bound, sync::Arc};

use jiff::{Timestamp, civil};
use rust_decimal::Decimal;
use serde_json::json;
use uuid::Uuid;

use crate::{
    ArrayType, BinOper, ColumnType, Interval, IntervalSpec, IpNetwork, JsonExistsBehavior,
    JsonQueryBehavior, JsonTableBehavior, JsonValueBehavior, MacAddress, Multirange, Name, Range,
    RangeType, StringLen, TypeName, Value, Vector,
};

/// Every variant of an open enum, one value each.
pub trait Variants: Sized + 'static {
    /// One value of each variant, in the order the enum declares them; a
    /// variant's payload is a value a binding translates, never SQL NULL.
    fn variants() -> Vec<Self>;

    /// The name the enum declares this value's variant by.
    fn variant_name(&self) -> &'static str;

    /// The names of the variants none of `values` is, in declaration order:
    /// what a binding that spells `values` leaves out.
    fn missing_from(values: &[Self]) -> Vec<&'static str> {
        Self::variants()
            .iter()
            .filter(|variant| {
                !values
                    .iter()
                    .any(|value| discriminant(value) == discriminant(*variant))
            })
            .map(Self::variant_name)
            .collect()
    }
}

/// `Variants` for `$ty` from one value of each variant. Inside pgorm-query
/// `variant_name` matches `$ty` exhaustively, so the list does not build
/// while it misses a variant, and `unreachable_patterns` refuses one it names
/// twice.
macro_rules! variants {
    ($ty:ident {
        $( $variant:ident $( ( $($arg:expr),* $(,)? ) )? $( { $($field:ident : $value:expr),* $(,)? } )? ),+ $(,)?
    }) => {
        impl Variants for $ty {
            fn variants() -> Vec<Self> {
                vec![$( $ty::$variant $( ( $($arg),* ) )? $( { $($field: $value),* } )? ),+]
            }

            fn variant_name(&self) -> &'static str {
                match self {
                    $( $ty::$variant { .. } => stringify!($variant), )+
                }
            }
        }
    };
}

fn int4_range() -> Range<Value> {
    Range::new(
        Bound::Included(Value::Int(Some(1))),
        Bound::Excluded(Value::Int(Some(5))),
    )
}

variants!(RangeType {
    Int4,
    Int8,
    Numeric,
    Date,
    Timestamp,
    TimestampTz,
});

variants!(ArrayType {
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
    Date,
    Time,
    DateTime,
    DateTimeWithTimeZone,
    Uuid,
    Decimal,
    IpNetwork,
    MacAddress,
    Vector,
    Interval,
    Range(RangeType::Int4),
    Multirange(RangeType::Int4),
});

variants!(Value {
    Bool(Some(true)),
    TinyInt(Some(-8)),
    SmallInt(Some(-16)),
    Int(Some(32)),
    BigInt(Some(-64)),
    Unsigned(Some(32)),
    BigUnsigned(Some(64)),
    Float(Some(1.5)),
    Double(Some(-2.5)),
    String(Some(Box::new("text".to_owned()))),
    Char(Some('c')),
    Bytes(Some(Box::new(vec![0, 255]))),
    Json(Some(Box::new(json!({"a": [1, null]})))),
    Date(Some(Box::new(civil::date(2024, 2, 29)))),
    Time(Some(Box::new(civil::time(12, 30, 15, 250_000_000)))),
    DateTime(Some(Box::new(civil::date(2024, 2, 29).at(12, 30, 15, 0)))),
    DateTimeWithTimeZone(Some(Box::new(Timestamp::UNIX_EPOCH))),
    Uuid(Some(Box::new(Uuid::nil()))),
    Decimal(Some(Box::new(Decimal::new(12_345, 2)))),
    Array(
        ArrayType::Int,
        Some(Box::new(vec![Value::Int(Some(1)), Value::Int(None)]))
    ),
    Vector(Some(Box::new(Vector::from(vec![1.0, -0.5])))),
    IpNetwork(Some(Box::new(IpNetwork::from(std::net::IpAddr::from([
        192, 0, 2, 7
    ]))))),
    MacAddress(Some(Box::new(MacAddress::new([0, 1, 2, 3, 4, 5])))),
    Interval(Some(Box::new(Interval::new(1, -2, 3)))),
    Range(RangeType::Int4, Some(Box::new(int4_range()))),
    Multirange(
        RangeType::Int4,
        Some(Box::new(Multirange::from(vec![int4_range()])))
    ),
});

variants!(ColumnType {
    Char(None),
    String(StringLen::None),
    Text,
    Bytea,
    SmallInteger,
    Integer,
    BigInteger,
    Float,
    Double,
    Decimal(None),
    Timestamp,
    TimestampWithTimeZone,
    Time,
    Date,
    Interval(IntervalSpec::Any(None)),
    Bit(None),
    VarBit(8),
    Boolean,
    Money,
    Json,
    JsonBinary,
    Uuid,
    Named(TypeName::new(Name::runtime("citext"))),
    Enum {
        name: Name::runtime("mood"),
        schema: Some(Name::runtime("app")),
        variants: vec![Name::runtime("calm")],
    },
    Array(Arc::new(ColumnType::Integer)),
    Vector(None),
    Cidr,
    Inet,
    MacAddr,
    LTree,
    Range(RangeType::Int4),
    Multirange(RangeType::Int4),
    CreatedRange {
        name: Name::runtime("floatrange"),
        schema: Some(Name::runtime("app")),
        subtype: Arc::new(ColumnType::Double),
    },
    CreatedMultirange {
        name: Name::runtime("floatmultirange"),
        schema: Some(Name::runtime("app")),
        subtype: Arc::new(ColumnType::Double),
    },
    Domain {
        name: Name::runtime("email"),
        schema: Some(Name::runtime("app")),
        base: Arc::new(ColumnType::Text),
    },
});

variants!(BinOper {
    And,
    Or,
    Like,
    NotLike,
    SimilarTo,
    NotSimilarTo,
    Is,
    IsNot,
    IsDistinctFrom,
    IsNotDistinctFrom,
    In,
    NotIn,
    Between,
    NotBetween,
    BetweenSymmetric,
    NotBetweenSymmetric,
    Equal,
    NotEqual,
    SmallerThan,
    GreaterThan,
    SmallerThanOrEqual,
    GreaterThanOrEqual,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    LShift,
    RShift,
    As,
    ILike,
    NotILike,
    Matches,
    Contains,
    Contained,
    Concatenate,
    Overlap,
    Similarity,
    WordSimilarity,
    StrictWordSimilarity,
    SimilarityDistance,
    WordSimilarityDistance,
    StrictWordSimilarityDistance,
    GetJsonField,
    CastJsonField,
    GetJsonPath,
    CastJsonPath,
    HasJsonKey,
    HasAnyJsonKeys,
    HasAllJsonKeys,
    JsonPathExists,
    Regex,
    RegexCaseInsensitive,
    NotRegex,
    NotRegexCaseInsensitive,
    NotExtendRightOf,
    NotExtendLeftOf,
    AdjacentTo,
    AtTimeZone,
    EuclideanDistance,
    NegativeInnerProduct,
    CosineDistance,
    Raw("OPERATOR(pg_catalog.+)"),
});

variants!(JsonExistsBehavior {
    True,
    False,
    Unknown,
    Error
});

variants!(JsonValueBehavior {
    Null,
    Error,
    Default(Value::Int(Some(0))),
});

variants!(JsonQueryBehavior {
    Null,
    Error,
    EmptyArray,
    EmptyObject,
    Default(Value::Json(Some(Box::new(json!([]))))),
});

variants!(JsonTableBehavior { Error, Empty });
