use bytes::BytesMut;
use jiff::civil::{Time, time};
use rust_decimal::Decimal;
use tokio_postgres::types::{FromSql, ToSql, Type};

use super::codecs::{CheckedArray, ExactDecimal};

type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

// [spec:pgorm:req:python.results/test]
#[test]
fn exact_numeric_keeps_rust_decimal_bytes() -> TestResult {
    for text in [
        "0.0000",
        "1.2300",
        "-42.001",
        "79228162514264337593543950335",
    ] {
        let expected = Decimal::from_str_exact(text)?;
        let mut raw = BytesMut::new();
        expected.to_sql(&Type::NUMERIC, &mut raw)?;
        // The Rust encoder canonicalizes zero scale on the wire.
        let rust = Decimal::from_sql(&Type::NUMERIC, &raw)?;
        let native = ExactDecimal::from_sql(&Type::NUMERIC, &raw)?.0;
        assert_eq!(native.serialize(), rust.serialize());
    }
    Ok(())
}

// [spec:pgorm:req:python.results/test]
#[test]
fn numeric_precision_loss_is_rejected() {
    // 1 + 1e-32: PostgreSQL base-10000 groups. Upstream Decimal rounds this.
    let words: [u16; 13] = [9, 0, 0, 32, 1, 0, 0, 0, 0, 0, 0, 0, 1];
    let raw: Vec<_> = words.iter().flat_map(|n| n.to_be_bytes()).collect();
    assert!(Decimal::from_sql(&Type::NUMERIC, &raw).is_ok());
    assert!(ExactDecimal::from_sql(&Type::NUMERIC, &raw).is_err());
    assert!(ExactDecimal::from_sql(&Type::NUMERIC, &[0; 7]).is_err());
}

// [spec:pgorm:req:python.results/test]
#[test]
fn array_codec_preserves_nullable_rust_elements() -> TestResult {
    let expected = vec![Some(1i32), None, Some(3)];
    let mut raw = BytesMut::new();
    expected.to_sql(&Type::INT4_ARRAY, &mut raw)?;
    assert_eq!(
        CheckedArray::<i32>::from_sql(&Type::INT4_ARRAY, &raw)?.0,
        expected
    );
    // The driver's Vec decoder ignores the first dimension's lower bound.
    raw[16..20].copy_from_slice(&0i32.to_be_bytes());
    assert_eq!(
        Vec::<Option<i32>>::from_sql(&Type::INT4_ARRAY, &raw)?,
        expected
    );
    assert!(CheckedArray::<i32>::from_sql(&Type::INT4_ARRAY, &raw).is_err());
    assert!(CheckedArray::<i32>::from_sql(&Type::INT4, &raw).is_err());
    Ok(())
}

// [spec:pgorm:req:python.results/test]
#[test]
fn time_decode_rejects_values_outside_a_civil_day() -> TestResult {
    let valid = 45_296_123_456i64.to_be_bytes();
    assert_eq!(
        Time::from_sql(&Type::TIME, &valid)?,
        time(12, 34, 56, 123_456_000)
    );
    // PostgreSQL admits 24:00:00; a civil time has no such hour.
    for outside in [86_400_000_000i64, -1, i64::MAX] {
        assert!(Time::from_sql(&Type::TIME, &outside.to_be_bytes()).is_err());
    }
    // Every accepted value re-encodes byte for byte, so decoding cannot round.
    let mut encoded = BytesMut::new();
    Time::from_sql(&Type::TIME, &valid)?.to_sql(&Type::TIME, &mut encoded)?;
    assert_eq!(valid.as_ref(), encoded.as_ref());
    Ok(())
}
