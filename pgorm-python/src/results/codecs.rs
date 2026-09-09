//! Checked adapters around the PostgreSQL driver's existing Rust codecs.

use std::error::Error;

use bytes::BytesMut;
use chrono::NaiveTime;
use fallible_iterator::FallibleIterator;
use pgorm::pgorm_query::{IpNetwork, MacAddress};
use rust_decimal::Decimal;
use tokio_postgres::types::{FromSql, Kind, ToSql, Type};

type CodecError = Box<dyn Error + Send + Sync>;

/// NaiveTime's driver codec wraps PostgreSQL 24:00:00 to midnight.
#[derive(Debug)]
pub(super) struct ExactTime(pub NaiveTime);

impl<'a> FromSql<'a> for ExactTime {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, CodecError> {
        let time = NaiveTime::from_sql(ty, raw)?;
        let mut encoded = BytesMut::new();
        time.to_sql(ty, &mut encoded)?;
        if raw != encoded.as_ref() {
            return Err("time cannot be represented exactly by Rust NaiveTime".into());
        }
        Ok(Self(time))
    }

    fn accepts(ty: &Type) -> bool {
        <NaiveTime as FromSql>::accepts(ty)
    }
}

/// The upstream Decimal decoder can round. Accept only exact values and scale.
// [spec:pgorm:req:python.results]
#[derive(Debug)]
pub(super) struct ExactDecimal(pub Decimal);

impl<'a> FromSql<'a> for ExactDecimal {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, CodecError> {
        let input = Numeric::read(raw)?;
        if input.scale > 28 {
            return Err("numeric scale exceeds Rust Decimal's exact range".into());
        }
        let value = Decimal::from_sql(ty, raw)?;
        let mut encoded = BytesMut::new();
        value.to_sql(ty, &mut encoded)?;
        let output = Numeric::read(&encoded)?;
        if value.scale() != u32::from(input.scale) || input.canonical() != output.canonical() {
            return Err("numeric cannot be represented exactly by Rust Decimal".into());
        }
        Ok(Self(value))
    }

    fn accepts(ty: &Type) -> bool {
        <Decimal as FromSql>::accepts(ty)
    }
}

struct Numeric {
    weight: i16,
    sign: u16,
    scale: u16,
    digits: Vec<u16>,
}

impl Numeric {
    fn read(raw: &[u8]) -> Result<Self, CodecError> {
        if raw.len() < 8 || !raw.len().is_multiple_of(2) {
            return Err("invalid numeric binary representation".into());
        }
        let word = |i| u16::from_be_bytes([raw[i], raw[i + 1]]);
        if usize::from(word(0)) * 2 + 8 != raw.len() {
            return Err("invalid numeric digit count".into());
        }
        let digits: Vec<_> = (8..raw.len()).step_by(2).map(word).collect();
        if digits.iter().any(|digit| *digit >= 10_000) || !matches!(word(4), 0 | 0x4000) {
            return Err("non-finite or invalid numeric is unsupported".into());
        }
        Ok(Self {
            weight: word(2) as i16,
            sign: word(4),
            scale: word(6),
            digits,
        })
    }

    fn canonical(&self) -> (i32, u16, &[u16]) {
        let start = self.digits.iter().position(|d| *d != 0);
        let end = self.digits.iter().rposition(|d| *d != 0);
        match (start, end) {
            (Some(start), Some(end)) => (
                i32::from(self.weight) - start as i32,
                self.sign,
                &self.digits[start..=end],
            ),
            _ => (0, 0, &[]),
        }
    }
}

/// Rust Value arrays cannot preserve extra dimensions or non-default bounds.
#[derive(Debug)]
pub(super) struct CheckedArray<T>(pub Vec<Option<T>>);

impl<'a, T: FromSql<'a>> FromSql<'a> for CheckedArray<T> {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, CodecError> {
        if !matches!(ty.kind(), Kind::Array(_)) {
            return Err("expected a PostgreSQL array".into());
        }
        let array = postgres_protocol::types::array_from_sql(raw)?;
        let dimensions = array.dimensions().collect::<Vec<_>>()?;
        if dimensions.len() > 1 || dimensions.first().is_some_and(|d| d.lower_bound != 1) {
            return Err("only one-dimensional arrays with lower bound 1 are supported".into());
        }
        Vec::<Option<T>>::from_sql(ty, raw).map(Self)
    }

    fn accepts(ty: &Type) -> bool {
        <Vec<Option<T>> as FromSql>::accepts(ty)
    }
}

#[derive(Debug)]
pub(super) struct EnumLabel(pub String);

impl<'a> FromSql<'a> for EnumLabel {
    fn from_sql(ty: &Type, raw: &'a [u8]) -> Result<Self, CodecError> {
        let label = std::str::from_utf8(raw)?;
        if !matches!(ty.kind(), Kind::Enum(labels) if labels.iter().any(|s| s == label)) {
            return Err("invalid enum label".into());
        }
        Ok(Self(label.to_owned()))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(ty.kind(), Kind::Enum(_))
    }
}

/// These types lack FromSql; delegate to the same protocol codecs as TryGetable.
#[derive(Debug)]
pub(super) struct Inet(pub IpNetwork);

impl<'a> FromSql<'a> for Inet {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Self, CodecError> {
        let inet = postgres_protocol::types::inet_from_sql(raw)?;
        Ok(Self(IpNetwork::new(inet.addr(), inet.netmask())?))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::INET | Type::CIDR)
    }
}

#[derive(Debug)]
pub(super) struct Mac(pub MacAddress);

impl<'a> FromSql<'a> for Mac {
    fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Self, CodecError> {
        Ok(Self(MacAddress::new(
            postgres_protocol::types::macaddr_from_sql(raw)?,
        )))
    }

    fn accepts(ty: &Type) -> bool {
        *ty == Type::MACADDR
    }
}
