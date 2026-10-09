//! PostgreSQL's `interval`, which pgorm's `Value` has no variant for.
//!
//! An interval is three independent fields — months, days and microseconds —
//! because a month and a day have no fixed length: `1 mon` is not `30 days`,
//! and `1 day` is not `24 hours` across a daylight-saving change. Each field
//! carries its own sign, so `1 mon -2 days` is a value. `Temporal.Duration`
//! requires every field to share one sign, so it cannot hold every interval;
//! the binding keeps the three fields as PostgreSQL does.

use std::error::Error;

use bytes::{BufMut, BytesMut};
use tokio_postgres::types::{FromSql, IsNull, Kind, ToSql, Type, to_sql_checked};

type CodecError = Box<dyn Error + Sync + Send>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Interval {
    pub(crate) months: i32,
    pub(crate) days: i32,
    pub(crate) microseconds: i64,
}

/// The type a value is written in: a domain is transparent on the wire.
pub(crate) fn wire_type(ty: &Type) -> &Type {
    let mut ty = ty;
    while let Kind::Domain(base) = ty.kind() {
        ty = base;
    }
    ty
}

/// The binary form, `interval_send`'s: the microseconds, then the days, then
/// the months.
// [spec:pgorm:req:napi.values]
impl<'a> FromSql<'a> for Interval {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Self, CodecError> {
        let (Some((microseconds, rest)), true) = (raw.split_first_chunk::<8>(), raw.len() == 16)
        else {
            return Err("an interval is sixteen bytes".into());
        };
        let (days, months) = rest.split_at(4);
        Ok(Self {
            microseconds: i64::from_be_bytes(*microseconds),
            days: i32::from_be_bytes(days.try_into()?),
            months: i32::from_be_bytes(months.try_into()?),
        })
    }

    fn accepts(ty: &Type) -> bool {
        *wire_type(ty) == Type::INTERVAL
    }
}

// [spec:pgorm:req:napi.values]
impl ToSql for Interval {
    fn to_sql(&self, ty: &Type, out: &mut BytesMut) -> Result<IsNull, CodecError> {
        if *wire_type(ty) != Type::INTERVAL {
            return Err(format!("cannot bind an `interval` value to Postgres type `{ty}`").into());
        }
        out.reserve(16);
        out.put_i64(self.microseconds);
        out.put_i32(self.days);
        out.put_i32(self.months);
        Ok(IsNull::No)
    }

    fn accepts(_: &Type) -> bool {
        true
    }

    to_sql_checked!();
}

#[cfg(test)]
mod tests {
    use super::*;

    // [spec:pgorm:req:napi.values/test]
    #[test]
    fn an_interval_round_trips_its_three_signed_fields() -> Result<(), CodecError> {
        let interval = Interval {
            months: 14,
            days: -3,
            microseconds: -(4 * 3_600_000_000 + 1),
        };
        let mut out = BytesMut::new();
        interval.to_sql(&Type::INTERVAL, &mut out)?;
        assert_eq!(out.len(), 16);
        assert_eq!(Interval::from_sql(&Type::INTERVAL, &out)?, interval);
        assert!(Interval::from_sql(&Type::INTERVAL, &out[..15]).is_err());
        assert!(interval.to_sql(&Type::INT8, &mut BytesMut::new()).is_err());
        Ok(())
    }
}
