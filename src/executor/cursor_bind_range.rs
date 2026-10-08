//! Binding a range or a multirange: each bound is written by the same adapter
//! as any other value, against the subtype the server says the range is
//! over, so a bound gets every coercion and every encoding fix a scalar of its
//! variant gets.

use std::ops::Bound;

use bytes::{BufMut, BytesMut};
use pgorm_query::{Multirange, Range};
use postgres_protocol::{
    IsNull as WireNull,
    types::{RangeBound, empty_range_to_sql, range_to_sql},
};

use super::*;

type WireResult<T> = Result<T, Box<dyn std::error::Error + Sync + Send>>;

/// Bind a range against a range type — a built-in one or a range type a
/// schema created, which tokio-postgres reports as a range over its subtype
/// all the same. Any other inferred type is refused.
// [spec:pgorm:req:exec.cursor.binding-range+1]
pub(super) fn bind_range(range: &Range<Value>, ty: &Type, out: &mut BytesMut) -> BindResult {
    match wire_type(ty).kind() {
        Kind::Range(subtype) => {
            write_range(range, subtype, out)?;
            Ok(IsNull::No)
        }
        _ => Err(mismatch("Range", ty)),
    }
}

/// Bind a multirange: a count, then each range length-prefixed in a range's
/// own encoding.
///
/// Only a built-in multirange can be bound. tokio-postgres learns a type's
/// kind from `pg_range` joined on the *range* type, so a multirange a schema
/// created comes back as a simple type with no subtype to write the bounds
/// in, and guessing one would send bytes the server reads as something else.
// [spec:pgorm:req:exec.cursor.binding-range+1]
pub(super) fn bind_multirange(
    multirange: &Multirange<Value>,
    ty: &Type,
    out: &mut BytesMut,
) -> BindResult {
    let Kind::Multirange(subtype) = wire_type(ty).kind() else {
        return Err(mismatch("Multirange", ty));
    };
    out.put_i32(i32::try_from(multirange.iter().count())?);
    for range in multirange.iter() {
        let base = out.len();
        out.put_i32(0);
        write_range(range, subtype, out)?;
        let len = i32::try_from(out.len() - base - 4)?;
        out[base..base + 4].copy_from_slice(&len.to_be_bytes());
    }
    Ok(IsNull::No)
}

/// A range's binary form: a flag byte, then each present bound
/// length-prefixed. The empty range is the flag byte alone.
// [spec:pgorm:req:exec.cursor.binding-range+1]
fn write_range(range: &Range<Value>, subtype: &Type, out: &mut BytesMut) -> WireResult<()> {
    match range {
        Range::Empty => {
            empty_range_to_sql(out);
            Ok(())
        }
        Range::Bounds { lower, upper } => range_to_sql(
            |out| write_bound(lower, subtype, out),
            |out| write_bound(upper, subtype, out),
            out,
        ),
    }
}

/// One bound, written through [`ValueHolder`] in the subtype's format.
///
/// A NULL bound is no bound, as PostgreSQL's range constructors read a NULL
/// argument and as the literal rendering writes one. Sent as a NULL, it would
/// reach the server as a bound of length -1, which the range's receive
/// function reads as a length rather than as a NULL.
// [spec:pgorm:req:exec.cursor.binding-range+1]
fn write_bound(
    bound: &Bound<Value>,
    subtype: &Type,
    out: &mut BytesMut,
) -> WireResult<RangeBound<WireNull>> {
    let (value, included) = match bound {
        Bound::Included(value) => (value, true),
        Bound::Excluded(value) => (value, false),
        Bound::Unbounded => return Ok(RangeBound::Unbounded),
    };
    Ok(match ValueHolder(value.clone()).to_sql(subtype, out)? {
        IsNull::Yes => RangeBound::Unbounded,
        IsNull::No if included => RangeBound::Inclusive(WireNull::No),
        IsNull::No => RangeBound::Exclusive(WireNull::No),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

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

    fn int4(value: i32) -> Vec<u8> {
        [&4i32.to_be_bytes()[..], &value.to_be_bytes()[..]].concat()
    }

    fn range(lower: Bound<Value>, upper: Bound<Value>) -> Value {
        Value::Range(
            pgorm_query::RangeType::Int4,
            Some(Box::new(Range::new(lower, upper))),
        )
    }

    // [spec:pgorm:req:exec.cursor.binding-range+1/test]
    #[test]
    fn binds_each_bound_with_its_inclusivity() {
        assert_eq!(
            bytes(Range::from(1i32..5).into(), &Type::INT4_RANGE),
            [&[0b0000_0010][..], &int4(1), &int4(5)].concat()
        );
        assert_eq!(
            bytes(Range::from(1i32..=5).into(), &Type::INT4_RANGE),
            [&[0b0000_0110][..], &int4(1), &int4(5)].concat()
        );
        assert_eq!(
            bytes(Range::from(..=5i32).into(), &Type::INT4_RANGE),
            [&[0b0000_1100][..], &int4(5)].concat()
        );
        assert_eq!(
            bytes(Range::<i32>::from(..).into(), &Type::INT4_RANGE),
            [0b0001_1000]
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-range+1/test]
    #[test]
    fn binds_the_empty_range_as_its_own_flag() {
        assert_eq!(
            bytes(Range::<i32>::Empty.into(), &Type::INT4_RANGE),
            [0b0000_0001]
        );
        assert_eq!(
            encode_as(
                Value::Range(pgorm_query::RangeType::Int4, None),
                &Type::INT4_RANGE
            ),
            Ok(None)
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-range+1/test]
    #[test]
    fn binds_a_null_bound_as_no_bound() {
        assert_eq!(
            bytes(
                range(
                    Bound::Included(Value::Int(None)),
                    Bound::Included(Value::Int(Some(5)))
                ),
                &Type::INT4_RANGE
            ),
            bytes(Range::from(..=5i32).into(), &Type::INT4_RANGE)
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-range+1/test]
    #[test]
    fn writes_bounds_through_the_scalar_adapter() {
        // An `int8` value narrowed to the range's `int4` subtype, as a scalar is.
        assert_eq!(
            bytes(
                range(
                    Bound::Included(Value::BigInt(Some(1))),
                    Bound::Excluded(Value::BigInt(Some(5)))
                ),
                &Type::INT4_RANGE
            ),
            [&[0b0000_0010][..], &int4(1), &int4(5)].concat()
        );
        // A zero keeps its scale, as a scalar `numeric` does.
        let zero = Decimal::new(0, 2);
        let encoded = bytes(Range::from(zero..).into(), &Type::NUM_RANGE);
        assert_eq!(&encoded[..5], &[0b0001_0010, 0, 0, 0, 8]);
        assert_eq!(&encoded[5..], &[0, 0, 0, 0, 0, 0, 0, 2]);
        assert_eq!(
            encode_as(
                range(
                    Bound::Included(Value::String(Some(Box::new("1".to_owned())))),
                    Bound::Unbounded
                ),
                &Type::INT4_RANGE
            ),
            Err("cannot bind a `String` value to Postgres type `int4`".to_owned())
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-range+1/test]
    #[test]
    fn binds_a_created_range_type_by_subtype() {
        let created = Type::new(
            "slot".to_owned(),
            16_384,
            Kind::Range(Type::INT4),
            "public".to_owned(),
        );
        assert_eq!(
            bytes(Range::from(1i32..5).into(), &created),
            bytes(Range::from(1i32..5).into(), &Type::INT4_RANGE)
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-range+1/test]
    #[test]
    fn binds_a_multirange_as_counted_ranges() {
        let multirange: Multirange<i32> = [Range::from(1..3), Range::Empty].into_iter().collect();
        let first = [&[0b0000_0010][..], &int4(1), &int4(3)].concat();
        assert_eq!(
            bytes(multirange.into(), &Type::INT4MULTI_RANGE),
            [
                &2i32.to_be_bytes()[..],
                &i32::try_from(first.len()).unwrap().to_be_bytes()[..],
                &first,
                &1i32.to_be_bytes()[..],
                &[0b0000_0001][..],
            ]
            .concat()
        );
        assert_eq!(
            bytes(Multirange::<i32>::default().into(), &Type::INT4MULTI_RANGE),
            0i32.to_be_bytes()
        );
    }

    // [spec:pgorm:req:exec.cursor.binding-range+1/test]
    #[test]
    fn refuses_a_range_against_other_types() {
        assert_eq!(
            encode_as(Range::from(1i32..5).into(), &Type::INT4),
            Err("cannot bind a `Range` value to Postgres type `int4`".to_owned())
        );
        assert_eq!(
            encode_as(Range::from(1i32..5).into(), &Type::INT4MULTI_RANGE),
            Err("cannot bind a `Range` value to Postgres type `int4multirange`".to_owned())
        );
        // A created multirange reaches the driver as a simple type.
        let created = Type::new(
            "slot_multirange".to_owned(),
            16_385,
            Kind::Simple,
            "public".to_owned(),
        );
        assert_eq!(
            encode_as(Multirange::<i32>::default().into(), &created),
            Err("cannot bind a `Multirange` value to Postgres type `slot_multirange`".to_owned())
        );
        assert_eq!(
            encode_as(Value::Int(Some(5)), &Type::INT4_RANGE),
            Err("cannot bind a `Int` value to Postgres type `int4range`".to_owned())
        );
    }
}
