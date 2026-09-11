//! Lossless tagged values: the Rust mirror of `pgorm_campaign.wire`.
//!
//! A value crossing into an observation carries three things — its Rust
//! variant, the qualified PostgreSQL identity of an enum when it has one, and a
//! payload chosen so that nothing is lost. Integers travel as decimal text
//! because JSON numbers are doubles; floats travel as IEEE bit patterns because
//! NaN payloads and signed zero are the values under test; `bytea` and MAC
//! addresses travel as integer arrays because a string escape would have to
//! pick an encoding.
//!
//! The Python oracle revalidates every payload with `wire.validate`, so an
//! encoding that drifts from `wire.py` does not render differently — it fails
//! parity. [`validate`] is that same check, reimplemented here so a malformed
//! observation is caught in the subject rather than in the oracle.

mod temporal;
mod validate;

pub use temporal::{
    parse_date, parse_datetime_fixed, parse_datetime_utc, parse_naive_datetime, parse_time,
    temporal_text,
};
pub use validate::validate;

use pgorm::pgorm_query::{ArrayType, Value};
use serde_json::{Value as Json, json};

use crate::FormatError;

/// An owned, identifier-only PostgreSQL type name, optionally schema qualified.
///
/// Only the parts of an identity that survive transport: a name and a schema,
/// never a rendered cast. Both are checked as identifiers when the tag carrying
/// them is validated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeName {
    /// The type's own name, unquoted.
    pub name: String,
    /// The schema qualifying it, unquoted, when the identity carries one.
    pub schema: Option<String>,
}

impl TypeName {
    /// An unqualified type name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            schema: None,
        }
    }

    /// The same name, qualified by a schema.
    #[must_use]
    pub fn in_schema(mut self, schema: impl Into<String>) -> Self {
        self.schema = Some(schema.into());
        self
    }
}

/// What a payload is, independent of the payload itself.
///
/// Nested arrays are absent by construction on the Python side and rejected by
/// [`validate`] here: PostgreSQL's own array type is one-dimensional as far as
/// the Rust value model is concerned.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tag {
    /// One of the kinds named in `wire.SCALARS`.
    Scalar(ArrayType),
    /// A PostgreSQL enum, identified by name and schema rather than by label set.
    Enum(TypeName),
    /// A one-dimensional array whose elements all carry the inner tag.
    Array(Box<Tag>),
}

impl Tag {
    /// An array of this tag's values.
    #[must_use]
    pub fn array(self) -> Self {
        Self::Array(Box::new(self))
    }

    /// The portable kind name, as it appears in the encoded `kind` field.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Scalar(kind) => scalar_name(kind),
            Self::Enum(_) => "enum",
            Self::Array(_) => "array",
        }
    }

    /// The encoded tag: `{"kind": ..}`, plus enum identity or element tag.
    pub fn encode(&self) -> Json {
        match self {
            Self::Scalar(_) => json!({"kind": self.name()}),
            Self::Enum(name) => json!({"kind": "enum", "name": name.name, "schema": name.schema}),
            Self::Array(element) => json!({"kind": "array", "element": element.encode()}),
        }
    }
}

macro_rules! scalar_kinds {
    ($( $variant:ident => $name:literal ),+ $(,)?) => {
        /// Every scalar kind name, matching `wire.SCALARS` less `enum`.
        pub const SCALAR_NAMES: &[&str] = &[$($name),+];

        /// The portable name of a Rust scalar variant.
        pub fn scalar_name(kind: &ArrayType) -> &'static str {
            match kind { $(ArrayType::$variant => $name),+ }
        }

        /// The scalar variant a portable name denotes.
        pub fn parse_scalar(name: &str) -> Option<ArrayType> {
            match name {
                $($name => Some(ArrayType::$variant),)+
                _ => None,
            }
        }

        /// The typed SQL NULL of a scalar kind.
        pub fn scalar_null(kind: &ArrayType) -> Value {
            match kind { $(ArrayType::$variant => Value::$variant(None)),+ }
        }

        /// The tag a Rust value carries on its own, before enum identity applies.
        pub fn rust_tag(value: &Value) -> Tag {
            match value {
                $(Value::$variant(_) => Tag::Scalar(ArrayType::$variant),)+
                Value::Array(kind, _) => Tag::Array(Box::new(Tag::Scalar(kind.clone()))),
            }
        }

        /// Whether the value is an SQL NULL rather than a present payload.
        ///
        /// Distinct from a JSON `null` payload, which is a present value whose
        /// content happens to be JSON's null literal.
        pub fn is_sql_null(value: &Value) -> bool {
            match value {
                $(Value::$variant(value) => value.is_none(),)+
                Value::Array(_, value) => value.is_none(),
            }
        }
    };
}

scalar_kinds! {
    Bool => "bool", TinyInt => "i8", SmallInt => "i16", Int => "i32",
    BigInt => "i64", Unsigned => "u32", BigUnsigned => "u64",
    Float => "f32", Double => "f64", String => "text", Char => "char",
    Bytes => "bytes", Json => "json", Decimal => "decimal", Uuid => "uuid",
    ChronoDate => "date", ChronoTime => "time", ChronoDateTime => "datetime",
    ChronoDateTimeUtc => "datetime_utc", ChronoDateTimeLocal => "datetime_local",
    ChronoDateTimeWithTimeZone => "datetime_fixed", IpNetwork => "ipnetwork",
    MacAddress => "mac_address", Vector => "vector",
}

/// A Rust value together with the identity its payload alone cannot carry.
///
/// `Value::String` is the Rust representation of both a `text` column and an
/// enum label; only the tag tells them apart, and the campaign's hostile enum
/// names make that distinction load-bearing.
#[derive(Clone, Debug)]
pub struct Tagged {
    inner: Value,
    tag: Tag,
}

impl Tagged {
    /// Wrap a Rust value, taking its tag from its own variant.
    pub fn from_value(inner: Value) -> Self {
        Self {
            tag: rust_tag(&inner),
            inner,
        }
    }

    /// Retain a PostgreSQL enum's qualified identity alongside its label payload.
    pub fn from_enum(inner: Value, name: TypeName, array: bool) -> Self {
        let tag = Tag::Enum(name);
        Self {
            inner,
            tag: if array { tag.array() } else { tag },
        }
    }

    /// Wrap a value under an explicitly chosen tag.
    pub fn with_tag(inner: Value, tag: Tag) -> Self {
        Self { inner, tag }
    }

    /// The value's tag.
    pub fn tag(&self) -> &Tag {
        &self.tag
    }

    /// The underlying Rust value.
    pub fn value(&self) -> &Value {
        &self.inner
    }

    /// Whether this is an SQL NULL rather than a present payload.
    pub fn is_sql_null(&self) -> bool {
        is_sql_null(&self.inner)
    }

    /// The tagged encoding: version, type tag, SQL NULL flag, payload.
    pub fn encode(&self) -> Json {
        json!({
            "version": 1,
            "type": self.tag.encode(),
            "sql_null": self.is_sql_null(),
            "data": self.payload(),
        })
    }

    /// [`encode`](Self::encode), rejected here rather than by the Python oracle
    /// if the payload falls outside what `wire.py` accepts.
    ///
    /// # Errors
    ///
    /// Returns [`FormatError`] when the encoding is not a valid portable value —
    /// a temporal beyond microsecond precision, a `bytea` past its byte budget,
    /// a JSON integer outside `i64`/`u64`.
    pub fn encode_checked(&self) -> Result<Json, FormatError> {
        let encoded = self.encode();
        validate(&encoded)?;
        Ok(encoded)
    }

    fn payload(&self) -> Json {
        macro_rules! string {
            ($value:expr) => {
                json!($value.as_ref().map(|value| value.to_string()))
            };
        }
        match &self.inner {
            Value::Bool(value) => json!(value),
            Value::TinyInt(value) => string!(value),
            Value::SmallInt(value) => string!(value),
            Value::Int(value) => string!(value),
            Value::BigInt(value) => string!(value),
            Value::Unsigned(value) => string!(value),
            Value::BigUnsigned(value) => string!(value),
            Value::Float(value) => json!(value.map(|value| format!("{:08x}", value.to_bits()))),
            Value::Double(value) => json!(value.map(|value| format!("{:016x}", value.to_bits()))),
            Value::String(value) => json!(value),
            Value::Char(value) => json!(value),
            Value::Bytes(value) => json!(value),
            Value::Json(value) => json!(value),
            Value::Decimal(value) => string!(value),
            Value::Uuid(value) => string!(value),
            Value::ChronoDate(value) => string!(value),
            Value::ChronoTime(value) => string!(value),
            Value::ChronoDateTime(value) => string!(value),
            Value::ChronoDateTimeUtc(value) => string!(value),
            Value::ChronoDateTimeLocal(value) => string!(value),
            Value::ChronoDateTimeWithTimeZone(value) => string!(value),
            Value::IpNetwork(value) => string!(value),
            Value::MacAddress(value) => json!(value.as_ref().map(|value| value.bytes())),
            Value::Vector(value) => json!(value.as_ref().map(|value| {
                value
                    .to_vec()
                    .iter()
                    .map(|value| format!("{:08x}", value.to_bits()))
                    .collect::<Vec<_>>()
            })),
            Value::Array(_, values) => json!(values.as_ref().map(|values| {
                values
                    .iter()
                    .map(|inner| {
                        let tag = match &self.tag {
                            Tag::Array(element) => (**element).clone(),
                            _ => rust_tag(inner),
                        };
                        Self {
                            inner: inner.clone(),
                            tag,
                        }
                        .encode()
                    })
                    .collect::<Vec<_>>()
            })),
        }
    }
}
