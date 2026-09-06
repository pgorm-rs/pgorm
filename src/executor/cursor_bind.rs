//! Binding a [`Value`] as a statement parameter, checked against the wire
//! format of the type PostgreSQL inferred for the placeholder.
//!
//! Split from `cursor.rs`: the bind adapter serves every executor, not just
//! cursors, and the acceptance matrix reads as one unit here.

use super::*;

/// Adapter binding a [`Value`] as a query parameter, converting it to the
/// Postgres type inferred for that placeholder — and refusing, before the
/// statement is sent, when the value has no representation that type could
/// receive.
// [spec:pgorm:def:exec.cursor.binding+4]
pub struct ValueHolder(pub Value);

impl std::fmt::Debug for ValueHolder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

use bytes::BytesMut;
use rust_decimal::Decimal;

type BindResult = Result<IsNull, Box<dyn std::error::Error + Sync + Send>>;

fn out_of_range(
    value: impl std::fmt::Display,
    ty: &Type,
) -> Box<dyn std::error::Error + Sync + Send> {
    format!("value `{value}` is out of range for Postgres type `{ty}`").into()
}

/// The refusal raised when a variant has no encoding that is the wire format
/// of the type Postgres inferred for the placeholder.
// [spec:pgorm:req:exec.cursor.binding-accepts]
fn mismatch(kind: &'static str, ty: &Type) -> Box<dyn std::error::Error + Sync + Send> {
    format!("cannot bind a `{kind}` value to Postgres type `{ty}`").into()
}

/// The type a value is actually written in. A domain is transparent on the
/// wire — its values are sent in the representation of the type it is built
/// over — so the whole binding decision, the acceptance check and the
/// encoding alike, is made against the base type.
// [spec:pgorm:req:exec.cursor.binding-accepts]
fn wire_type(ty: &Type) -> &Type {
    let mut ty = ty;
    while let Kind::Domain(base) = ty.kind() {
        ty = base;
    }
    ty
}

/// The types whose binary representation is the value's text: the four
/// built-in string types, every enum (a label is sent as its own text),
/// `xml`, `unknown`, and the text-backed extension types `postgres-types`
/// itself names.
// [spec:pgorm:req:exec.cursor.binding-accepts]
fn is_textual(ty: &Type) -> bool {
    matches!(
        *ty,
        Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::XML | Type::UNKNOWN
    ) || matches!(ty.kind(), Kind::Enum(_))
        || matches!(ty.name(), "citext" | "ltree" | "lquery" | "ltxtquery")
}

/// `timestamp` and `timestamptz` share one representation — microseconds since
/// 2000-01-01 — and differ only in whether that instant is read as a wall
/// clock or as UTC. An acceptance check is about representation, so both are
/// accepted for every chrono variant, exactly as `postgres-types` does for its
/// own `SystemTime` impl. Which of the two a value *means* is the caller's.
// [spec:pgorm:req:exec.cursor.binding-accepts]
fn is_timestamp(ty: &Type) -> bool {
    matches!(*ty, Type::TIMESTAMP | Type::TIMESTAMPTZ)
}

/// Bind a payload whose binary encoding is the wire format of the `accepted`
/// types and of no others.
///
/// A `None` payload is SQL `NULL`, which is sent as a length of -1 with no
/// bytes at all. Having no representation, it has none to mismatch, so it
/// binds against whatever type Postgres inferred.
// [spec:pgorm:req:exec.cursor.binding-accepts]
fn bind_exact<T>(
    value: Option<&T>,
    kind: &'static str,
    accepted: fn(&Type) -> bool,
    ty: &Type,
    out: &mut BytesMut,
) -> BindResult
where
    T: ToSql,
{
    let target = wire_type(ty);
    match value {
        None => Ok(IsNull::Yes),
        Some(value) if accepted(target) => value.to_sql(target, out),
        Some(_) => Err(mismatch(kind, ty)),
    }
}

/// Bind an integer against the type Postgres inferred for the placeholder.
/// Within the numeric family the value is converted and written in *that*
/// type's format; `oid` and `"char"` take the widths their own impls define,
/// through the same checked conversion. Any other inferred type has no
/// integer representation to receive, and is refused.
// [spec:pgorm:req:exec.cursor.binding-coerce+2]
fn integer_to_sql(value: i64, kind: &'static str, ty: &Type, out: &mut BytesMut) -> BindResult {
    let target = wire_type(ty);
    if *target == Type::INT2 {
        match i16::try_from(value) {
            Ok(value) => value.to_sql(target, out),
            Err(_) => Err(out_of_range(value, ty)),
        }
    } else if *target == Type::INT4 {
        match i32::try_from(value) {
            Ok(value) => value.to_sql(target, out),
            Err(_) => Err(out_of_range(value, ty)),
        }
    } else if *target == Type::INT8 {
        value.to_sql(target, out)
    } else if *target == Type::OID {
        match u32::try_from(value) {
            Ok(value) => value.to_sql(target, out),
            Err(_) => Err(out_of_range(value, ty)),
        }
    } else if *target == Type::CHAR {
        match i8::try_from(value) {
            Ok(value) => value.to_sql(target, out),
            Err(_) => Err(out_of_range(value, ty)),
        }
    } else if *target == Type::FLOAT4 {
        (value as f32).to_sql(target, out)
    } else if *target == Type::FLOAT8 {
        (value as f64).to_sql(target, out)
    } else if *target == Type::NUMERIC {
        Decimal::from(value).to_sql(target, out)
    } else {
        Err(mismatch(kind, ty))
    }
}

/// The floating-point counterpart of [`integer_to_sql`]. Narrowing to `float4`
/// rounds the way Postgres' own `float8 -> float4` cast does, but a conversion
/// that would silently drop the fractional part or overflow is an error rather
/// than a lie.
// [spec:pgorm:req:exec.cursor.binding-coerce+2]
fn float_to_sql(value: f64, kind: &'static str, ty: &Type, out: &mut BytesMut) -> BindResult {
    let target = wire_type(ty);
    if *target == Type::FLOAT4 {
        let narrowed = value as f32;
        if value.is_finite() && !narrowed.is_finite() {
            Err(out_of_range(value, ty))
        } else {
            narrowed.to_sql(target, out)
        }
    } else if *target == Type::FLOAT8 {
        value.to_sql(target, out)
    } else if *target == Type::NUMERIC {
        match Decimal::try_from(value) {
            Ok(value) => value.to_sql(target, out),
            Err(_) => Err(out_of_range(value, ty)),
        }
    } else if *target == Type::INT2 || *target == Type::INT4 || *target == Type::INT8 {
        Err(format!(
            "cannot bind floating-point value `{value}` to Postgres type `{ty}` without loss"
        )
        .into())
    } else {
        Err(mismatch(kind, ty))
    }
}

// [spec:pgorm:req:exec.cursor.binding-coerce+2]
fn bind_integer<T>(
    value: Option<T>,
    kind: &'static str,
    ty: &Type,
    out: &mut BytesMut,
) -> BindResult
where
    T: Copy + Into<i64>,
{
    match value {
        None => Ok(IsNull::Yes),
        Some(value) => integer_to_sql(value.into(), kind, ty, out),
    }
}

// [spec:pgorm:req:exec.cursor.binding-coerce+2]
fn bind_float<T>(value: Option<T>, kind: &'static str, ty: &Type, out: &mut BytesMut) -> BindResult
where
    T: Copy + Into<f64>,
{
    match value {
        None => Ok(IsNull::Yes),
        Some(value) => float_to_sql(value.into(), kind, ty, out),
    }
}

/// `u64` is the one integer variant with no `Into<i64>`: Postgres has no
/// unsigned 64-bit type, so the value has to fit `i64` to be written at all.
// [spec:pgorm:req:exec.cursor.binding-coerce+2]
fn bind_big_unsigned(value: Option<u64>, ty: &Type, out: &mut BytesMut) -> BindResult {
    match value {
        None => Ok(IsNull::Yes),
        Some(value) => match i64::try_from(value) {
            Ok(value) => bind_integer(Some(value), "BigUnsigned", ty, out),
            Err(_) => Err(out_of_range(value, ty)),
        },
    }
}

// [spec:pgorm:def:exec.cursor.binding+4]
impl ToSql for ValueHolder {
    // [spec:pgorm:req:exec.cursor.binding-gaps+3]
    fn to_sql(
        &self,
        ty: &Type,
        out: &mut BytesMut,
    ) -> Result<tokio_postgres::types::IsNull, Box<dyn std::error::Error + Sync + Send>>
    where
        Self: Sized,
    {
        match &self.0 {
            Value::Bool(x) => bind_exact(x.as_ref(), "Bool", |ty| *ty == Type::BOOL, ty, out),
            Value::TinyInt(x) => bind_integer(*x, "TinyInt", ty, out),
            Value::SmallInt(x) => bind_integer(*x, "SmallInt", ty, out),
            Value::Int(x) => bind_integer(*x, "Int", ty, out),
            Value::BigInt(x) => bind_integer(*x, "BigInt", ty, out),
            Value::Unsigned(x) => bind_integer(*x, "Unsigned", ty, out),
            Value::BigUnsigned(x) => bind_big_unsigned(*x, ty, out),
            Value::Float(x) => bind_float(*x, "Float", ty, out),
            Value::Double(x) => bind_float(*x, "Double", ty, out),
            Value::String(x) => bind_exact(x.as_deref(), "String", is_textual, ty, out),
            Value::Char(x) => {
                let x = x.map(|x| x.to_string());
                bind_exact(x.as_ref(), "Char", is_textual, ty, out)
            }
            Value::Bytes(x) => bind_exact(x.as_deref(), "Bytes", |ty| *ty == Type::BYTEA, ty, out),
            Value::Json(x) => bind_exact(
                x.as_deref(),
                "Json",
                |ty| matches!(*ty, Type::JSON | Type::JSONB),
                ty,
                out,
            ),
            Value::ChronoDate(x) => {
                bind_exact(x.as_deref(), "ChronoDate", |ty| *ty == Type::DATE, ty, out)
            }
            Value::ChronoTime(x) => {
                bind_exact(x.as_deref(), "ChronoTime", |ty| *ty == Type::TIME, ty, out)
            }
            Value::ChronoDateTime(x) => {
                bind_exact(x.as_deref(), "ChronoDateTime", is_timestamp, ty, out)
            }
            Value::ChronoDateTimeUtc(x) => {
                bind_exact(x.as_deref(), "ChronoDateTimeUtc", is_timestamp, ty, out)
            }
            Value::ChronoDateTimeLocal(x) => {
                bind_exact(x.as_deref(), "ChronoDateTimeLocal", is_timestamp, ty, out)
            }
            Value::ChronoDateTimeWithTimeZone(x) => bind_exact(
                x.as_deref(),
                "ChronoDateTimeWithTimeZone",
                is_timestamp,
                ty,
                out,
            ),
            Value::Uuid(x) => bind_exact(x.as_deref(), "Uuid", |ty| *ty == Type::UUID, ty, out),
            Value::Decimal(x) => {
                bind_exact(x.as_deref(), "Decimal", |ty| *ty == Type::NUMERIC, ty, out)
            }
            Value::Array(_, Some(x)) => {
                // `Vec<T>`'s own impl panics on a non-array type rather than
                // erroring, so the shape has to be established here first.
                // Each element then re-enters this impl with the member type.
                let target = wire_type(ty);
                match target.kind() {
                    Kind::Array(_) => x
                        .iter()
                        .map(|x| ValueHolder(x.clone()))
                        .collect::<Vec<_>>()
                        .to_sql(target, out),
                    _ => Err(mismatch("Array", ty)),
                }
            }
            Value::Array(_, None) => Ok(IsNull::Yes),
            Value::Vector(x) => {
                bind_exact(x.as_deref(), "Vector", |ty| ty.name() == "vector", ty, out)
            }
            Value::IpNetwork(x) => match x.as_deref() {
                None => Ok(IsNull::Yes),
                Some(_) if !matches!(*wire_type(ty), Type::INET | Type::CIDR) => {
                    Err(mismatch("IpNetwork", ty))
                }
                Some(x) => {
                    postgres_protocol::types::inet_to_sql(x.ip(), x.prefix(), out);
                    Ok(IsNull::No)
                }
            },
            Value::MacAddress(x) => match x.as_deref() {
                None => Ok(IsNull::Yes),
                Some(_) if *wire_type(ty) != Type::MACADDR => Err(mismatch("MacAddress", ty)),
                Some(x) => {
                    postgres_protocol::types::macaddr_to_sql(x.bytes(), out);
                    Ok(IsNull::No)
                }
            },
        }
    }

    /// Every Postgres type is accepted here because this is the wrong place to
    /// refuse one: `accepts` is a static method, with no access to the `Value`
    /// whose representation is the question. The acceptance decision of
    /// `[spec:pgorm:req:exec.cursor.binding-accepts]` therefore lives in
    /// `to_sql`, which is reached through `to_sql_checked!` and runs
    /// client-side before the bind message is sent.
    // [spec:pgorm:req:exec.cursor.binding-accepts]
    fn accepts(_ty: &Type) -> bool
    where
        Self: Sized,
    {
        true
    }

    to_sql_checked!();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for a type `postgres-types` has no constant for: an
    /// extension type (`vector`), an enum, or a domain over another type.
    fn named(name: &str, kind: Kind) -> Type {
        Type::new(name.to_owned(), 16_384, kind, "public".to_owned())
    }

    fn encode_as(value: Value, ty: &Type) -> Result<Option<Vec<u8>>, String> {
        let mut out = BytesMut::new();
        match ValueHolder(value).to_sql(ty, &mut out) {
            Ok(IsNull::Yes) => Ok(None),
            Ok(IsNull::No) => Ok(Some(out.to_vec())),
            Err(err) => Err(err.to_string()),
        }
    }

    fn bytes(value: Value, ty: &Type) -> Vec<u8> {
        encode_as(value, ty).unwrap().unwrap()
    }

    fn error(value: Value, ty: &Type) -> String {
        encode_as(value, ty).unwrap_err()
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce+2/test]
    #[test]
    fn binds_integer_as_float() {
        assert_eq!(
            bytes(Value::Int(Some(2)), &Type::FLOAT8),
            2.0f64.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::Int(Some(2)), &Type::FLOAT4),
            2.0f32.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::BigInt(Some(-3)), &Type::FLOAT8),
            (-3.0f64).to_be_bytes()
        );
        assert_eq!(encode_as(Value::Int(None), &Type::FLOAT8), Ok(None));
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce+2/test]
    #[test]
    fn binds_integer_across_widths() {
        assert_eq!(
            bytes(Value::BigInt(Some(300)), &Type::INT2),
            300i16.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::SmallInt(Some(7)), &Type::INT8),
            7i64.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::TinyInt(Some(-1)), &Type::INT4),
            (-1i32).to_be_bytes()
        );
        assert_eq!(
            bytes(Value::BigUnsigned(Some(9)), &Type::INT2),
            9i16.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::Unsigned(Some(9)), &Type::OID),
            9u32.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::TinyInt(Some(65)), &Type::CHAR),
            65i8.to_be_bytes()
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce+2/test]
    #[test]
    fn binds_integer_as_numeric() {
        assert_eq!(
            bytes(Value::Int(Some(2)), &Type::NUMERIC),
            vec![0, 1, 0, 0, 0, 0, 0, 0, 0, 2]
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce+2/test]
    #[test]
    fn rejects_out_of_range_integer() {
        assert_eq!(
            error(Value::BigInt(Some(i64::from(i32::MAX) + 1)), &Type::INT4),
            "value `2147483648` is out of range for Postgres type `int4`"
        );
        assert_eq!(
            error(Value::Int(Some(40_000)), &Type::INT2),
            "value `40000` is out of range for Postgres type `int2`"
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce+2/test]    a `u64` above
    // `i64::MAX` has no `int8` to be written in, so it errors rather than
    // wrapping to a negative — whatever type Postgres inferred
    #[test]
    fn rejects_out_of_range_big_unsigned() {
        let too_big = i64::MAX as u64 + 1;

        for ty in [
            &Type::INT8,
            &Type::INT4,
            &Type::INT2,
            &Type::NUMERIC,
            &Type::FLOAT8,
            &Type::TEXT,
        ] {
            assert_eq!(
                error(Value::BigUnsigned(Some(too_big)), ty),
                format!("value `{too_big}` is out of range for Postgres type `{ty}`")
            );
        }

        assert_eq!(
            error(Value::BigUnsigned(Some(u64::MAX)), &Type::INT8),
            "value `18446744073709551615` is out of range for Postgres type `int8`"
        );

        // The largest value that does fit is written exactly, not approximated.
        assert_eq!(
            bytes(Value::BigUnsigned(Some(i64::MAX as u64)), &Type::INT8),
            i64::MAX.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::BigUnsigned(Some(7)), &Type::FLOAT8),
            7.0f64.to_be_bytes()
        );
        assert_eq!(encode_as(Value::BigUnsigned(None), &Type::INT8), Ok(None));
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce+2/test]
    #[test]
    fn binds_float_across_widths() {
        assert_eq!(
            bytes(Value::Float(Some(1.5)), &Type::FLOAT8),
            1.5f64.to_be_bytes()
        );
        assert_eq!(
            bytes(Value::Double(Some(1.5)), &Type::FLOAT4),
            1.5f32.to_be_bytes()
        );
        assert!(
            error(Value::Double(Some(1e300)), &Type::FLOAT4)
                .ends_with("is out of range for Postgres type `float4`")
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce+2/test]
    #[test]
    fn rejects_float_bound_to_integer() {
        assert_eq!(
            error(Value::Double(Some(1.5)), &Type::INT8),
            "cannot bind floating-point value `1.5` to Postgres type `int8` without loss"
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-gaps+3/test]
    #[test]
    fn binds_vector() {
        let vector = named("vector", Kind::Simple);
        assert_eq!(
            bytes(
                Value::Vector(Some(Box::new(pgvector::Vector::from(vec![1.0f32, 2.0])))),
                &vector
            ),
            [
                &[0, 2, 0, 0][..],
                &1.0f32.to_be_bytes()[..],
                &2.0f32.to_be_bytes()[..],
            ]
            .concat()
        );
        assert_eq!(encode_as(Value::Vector(None), &vector), Ok(None));
    }

    // [spec:pgorm:req:exec.cursor.binding-gaps+3/test]
    #[test]
    fn binds_ip_network() {
        assert_eq!(
            bytes(
                Value::IpNetwork(Some(Box::new("10.0.0.1/24".parse().unwrap()))),
                &Type::INET
            ),
            vec![2, 24, 0, 4, 10, 0, 0, 1]
        );
        assert_eq!(encode_as(Value::IpNetwork(None), &Type::INET), Ok(None));
    }

    // [spec:pgorm:req:exec.cursor.binding-gaps+3/test]
    #[test]
    fn binds_mac_address() {
        assert_eq!(
            bytes(
                Value::MacAddress(Some(Box::new("00:11:22:33:44:55".parse().unwrap()))),
                &Type::MACADDR
            ),
            vec![0x00, 0x11, 0x22, 0x33, 0x44, 0x55]
        );
        assert_eq!(encode_as(Value::MacAddress(None), &Type::MACADDR), Ok(None));
    }

    /// The defect this check exists for: ASCII digits written into an `int4`
    /// placeholder were read back as the integer those bytes spell.
    // [spec:pgorm:req:exec.cursor.binding-accepts/test]
    #[test]
    fn rejects_string_bound_to_non_textual_types() {
        for ty in [
            &Type::INT4,
            &Type::INT8,
            &Type::FLOAT8,
            &Type::NUMERIC,
            &Type::BOOL,
            &Type::BYTEA,
            &Type::UUID,
            &Type::DATE,
            &Type::TIMESTAMPTZ,
            &Type::JSONB,
            &Type::INET,
        ] {
            assert_eq!(
                error(Value::String(Some(Box::new("1234".to_owned()))), ty),
                format!("cannot bind a `String` value to Postgres type `{ty}`")
            );
        }
    }

    /// Every type whose binary representation *is* the text keeps working.
    // [spec:pgorm:req:exec.cursor.binding-accepts/test]
    #[test]
    fn binds_string_to_textual_types() {
        let mood = named("mood", Kind::Enum(vec!["happy".to_owned()]));
        for ty in [
            &Type::TEXT,
            &Type::VARCHAR,
            &Type::BPCHAR,
            &Type::NAME,
            &Type::XML,
            &Type::UNKNOWN,
            &mood,
            &named("citext", Kind::Simple),
        ] {
            assert_eq!(
                bytes(Value::String(Some(Box::new("happy".to_owned()))), ty),
                b"happy"
            );
        }
        assert_eq!(bytes(Value::Char(Some('h')), &mood), b"h");

        // `ltree` and its query types are accepted too, but `postgres-types`
        // writes them with a leading format-version byte rather than as bare
        // text, so the bytes are its business, not this check's.
        for name in ["ltree", "lquery", "ltxtquery"] {
            let ty = named(name, Kind::Simple);
            assert_eq!(
                bytes(Value::String(Some(Box::new("a.b".to_owned()))), &ty),
                [&[1u8][..], b"a.b"].concat()
            );
        }
    }

    /// A domain is transparent on the wire, so the decision is made against
    /// the type it is built over — in both directions.
    // [spec:pgorm:req:exec.cursor.binding-accepts/test]
    #[test]
    fn resolves_domains_to_their_base_type() {
        let email = named("email", Kind::Domain(Type::TEXT));
        let positive = named("positive", Kind::Domain(Type::INT4));

        assert_eq!(
            bytes(Value::String(Some(Box::new("a@b".to_owned()))), &email),
            b"a@b"
        );
        assert_eq!(bytes(Value::BigInt(Some(7)), &positive), 7i32.to_be_bytes());

        // The refusal names the type the schema declares, not the base type
        // the check was made against.
        assert_eq!(
            error(Value::String(Some(Box::new("7".to_owned()))), &positive),
            "cannot bind a `String` value to Postgres type `positive`"
        );
        assert_eq!(
            error(Value::Int(Some(7)), &email),
            "cannot bind a `Int` value to Postgres type `email`"
        );
        assert_eq!(
            error(Value::BigInt(Some(i64::MAX)), &positive),
            format!(
                "value `{}` is out of range for Postgres type `positive`",
                i64::MAX
            )
        );
    }

    /// A domain over a domain resolves all the way down, and a `jsonb` domain
    /// still gets the version byte its base type's encoding prescribes.
    // [spec:pgorm:req:exec.cursor.binding-accepts/test]
    #[test]
    fn resolves_nested_domains() {
        let inner = named("inner", Kind::Domain(Type::JSONB));
        let outer = named("outer", Kind::Domain(inner));
        assert_eq!(
            bytes(Value::Json(Some(Box::new(serde_json::json!(1)))), &outer),
            b"\x011"
        );
    }

    /// `NULL` is sent as a length of -1 with no bytes, so it has no
    /// representation to mismatch and binds against any inferred type.
    // [spec:pgorm:req:exec.cursor.binding-accepts/test]
    #[test]
    fn binds_null_against_any_type() {
        for value in [
            Value::String(None),
            Value::Int(None),
            Value::Json(None),
            Value::Uuid(None),
            Value::Bytes(None),
            Value::Array(pgorm_query::ArrayType::Int, None),
        ] {
            for ty in [&Type::INT4, &Type::TEXT, &Type::BYTEA, &Type::UUID] {
                assert_eq!(encode_as(value.clone(), ty), Ok(None));
            }
        }
    }

    // [spec:pgorm:req:exec.cursor.binding-accepts/test]
    #[test]
    fn rejects_mismatched_payload_variants() {
        let cases = [
            (Value::Bool(Some(true)), &Type::INT4, "Bool"),
            (Value::Bytes(Some(Box::new(vec![1]))), &Type::TEXT, "Bytes"),
            (
                Value::Json(Some(Box::new(serde_json::json!(1)))),
                &Type::TEXT,
                "Json",
            ),
            (
                Value::Uuid(Some(Box::new(uuid::Uuid::nil()))),
                &Type::TEXT,
                "Uuid",
            ),
            (
                Value::Decimal(Some(Box::new(Decimal::ONE))),
                &Type::FLOAT8,
                "Decimal",
            ),
            (Value::Int(Some(1)), &Type::BOOL, "Int"),
            (Value::Double(Some(1.5)), &Type::TEXT, "Double"),
            (
                Value::ChronoDate(Some(Box::new(
                    chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("a real date"),
                ))),
                &Type::TIMESTAMP,
                "ChronoDate",
            ),
            (
                Value::IpNetwork(Some(Box::new(
                    "10.0.0.1/24".parse().expect("a real network"),
                ))),
                &Type::BYTEA,
                "IpNetwork",
            ),
            (
                Value::MacAddress(Some(Box::new(
                    "00:11:22:33:44:55".parse().expect("a real address"),
                ))),
                &Type::BYTEA,
                "MacAddress",
            ),
        ];

        for (value, ty, kind) in cases {
            assert_eq!(
                error(value, ty),
                format!("cannot bind a `{kind}` value to Postgres type `{ty}`")
            );
        }
    }

    /// `timestamp` and `timestamptz` share a representation, so every chrono
    /// datetime variant binds against either.
    // [spec:pgorm:req:exec.cursor.binding-accepts/test]
    #[test]
    fn binds_datetimes_to_either_timestamp_type() {
        let naive = chrono::NaiveDate::from_ymd_opt(2000, 1, 1)
            .expect("a real date")
            .and_hms_opt(0, 0, 1)
            .expect("a real time");
        let micros = 1_000_000i64.to_be_bytes();

        for ty in [&Type::TIMESTAMP, &Type::TIMESTAMPTZ] {
            assert_eq!(
                bytes(Value::ChronoDateTime(Some(Box::new(naive))), ty),
                micros
            );
            assert_eq!(
                bytes(
                    Value::ChronoDateTimeUtc(Some(Box::new(naive.and_utc()))),
                    ty
                ),
                micros
            );
        }

        assert_eq!(
            error(Value::ChronoDateTime(Some(Box::new(naive))), &Type::DATE),
            "cannot bind a `ChronoDateTime` value to Postgres type `date`"
        );
    }

    /// An array binds only against an array type, and its elements are held
    /// to the member type — a mismatched element is refused, not written.
    /// The non-array target used to reach a `panic!` inside `postgres-types`.
    // [spec:pgorm:req:exec.cursor.binding-accepts/test]
    #[test]
    fn checks_arrays_against_the_member_type() {
        use pgorm_query::ArrayType;

        let text_array = Value::Array(
            ArrayType::String,
            Some(Box::new(vec![Value::String(Some(Box::new(
                "a".to_owned(),
            )))])),
        );

        assert!(encode_as(text_array.clone(), &Type::TEXT_ARRAY).is_ok());
        assert_eq!(
            error(text_array.clone(), &Type::TEXT),
            "cannot bind a `Array` value to Postgres type `text`"
        );
        assert_eq!(
            error(text_array, &Type::INT4_ARRAY),
            "cannot bind a `String` value to Postgres type `int4`"
        );

        // The element-wise numeric coercion still applies through the member.
        let int_array = Value::Array(ArrayType::Int, Some(Box::new(vec![Value::Int(Some(2))])));
        assert!(encode_as(int_array, &Type::FLOAT8_ARRAY).is_ok());
    }

    // [spec:pgorm:req:exec.cursor.binding-coerce+2/test]
    #[test]
    fn rejects_integer_bound_outside_the_numeric_family() {
        for ty in [&Type::TEXT, &Type::BYTEA, &Type::BOOL, &Type::UUID] {
            assert_eq!(
                error(Value::Int(Some(1)), ty),
                format!("cannot bind a `Int` value to Postgres type `{ty}`")
            );
        }
        assert_eq!(
            error(Value::BigInt(Some(-1)), &Type::OID),
            "value `-1` is out of range for Postgres type `oid`"
        );
        assert_eq!(
            error(Value::Int(Some(300)), &Type::CHAR),
            format!(
                "value `300` is out of range for Postgres type `{}`",
                Type::CHAR
            )
        );
    }
}
