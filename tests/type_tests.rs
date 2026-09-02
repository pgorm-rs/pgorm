#![allow(unused_imports, dead_code)]

pub mod common;

use pgorm::{IntoActiveValue, TryFromU64, TryGetable, Value};

/*

When supporting a new type in pgorm we should implement the following traits for it:
  - `IntoActiveValue`, given that it implemented `Into<Value>` already
  - `TryGetable`
  - `TryFromU64`

Also, we need to update `impl FromQueryResult for JsonValue` at `src/query/json.rs`
to correctly serialize the type as `serde_json::Value`.

*/

pub fn it_impl_into_active_value<T: IntoActiveValue<V>, V: Into<Value>>() {}

pub fn it_impl_try_getable<T: TryGetable>() {}

pub fn it_impl_try_from_u64<T: TryFromU64>() {}

#[allow(unused_macros)]
macro_rules! it_impl_traits {
    ( $ty: ty ) => {
        it_impl_into_active_value::<$ty, $ty>();
        it_impl_into_active_value::<Option<$ty>, Option<$ty>>();
        it_impl_into_active_value::<Option<Option<$ty>>, Option<$ty>>();

        it_impl_try_getable::<$ty>();
        it_impl_try_getable::<Option<$ty>>();

        it_impl_try_from_u64::<$ty>();
    };
}

#[pgorm_macros::test]
fn main() {
    it_impl_traits!(i8);
    it_impl_traits!(i16);
    it_impl_traits!(i32);
    it_impl_traits!(i64);
    it_impl_traits!(u32);
    it_impl_traits!(bool);
    it_impl_traits!(f32);
    it_impl_traits!(f64);
    it_impl_traits!(Vec<u8>);
    it_impl_traits!(String);
    it_impl_traits!(serde_json::Value);
    it_impl_traits!(chrono::NaiveDate);
    it_impl_traits!(chrono::NaiveTime);
    it_impl_traits!(chrono::NaiveDateTime);
    it_impl_traits!(chrono::DateTime<chrono::FixedOffset>);
    it_impl_traits!(chrono::DateTime<chrono::Utc>);
    it_impl_traits!(chrono::DateTime<chrono::Local>);
    it_impl_traits!(rust_decimal::Decimal);
    it_impl_traits!(uuid::Uuid);
}

// [spec:pgorm:def:exec.decode.from-u64+1/test]    checked numeric conversion,
// `String` via `to_string`, and the unconditional refusal everywhere else
#[pgorm_macros::test]
fn try_from_u64_conversions() {
    use pgorm::DbErr;

    // The numeric impls go through a checked `TryInto`...
    assert_eq!(i8::try_from_u64(127).unwrap(), 127);
    assert_eq!(i16::try_from_u64(32_767).unwrap(), 32_767);
    assert_eq!(i32::try_from_u64(7).unwrap(), 7);
    assert_eq!(
        i64::try_from_u64(u64::from(u32::MAX)).unwrap(),
        4_294_967_295
    );
    assert_eq!(u8::try_from_u64(255).unwrap(), 255);
    assert_eq!(u16::try_from_u64(65_535).unwrap(), 65_535);
    assert_eq!(u32::try_from_u64(4_294_967_295).unwrap(), u32::MAX);
    assert_eq!(u64::try_from_u64(u64::MAX).unwrap(), u64::MAX);

    // ... and report `TryIntoErr` — naming both ends of the conversion — when
    // the value does not fit.
    assert!(matches!(
        i8::try_from_u64(128),
        Err(DbErr::TryIntoErr {
            from: "u64",
            into: "i8",
            ..
        })
    ));
    assert!(matches!(
        i32::try_from_u64(u64::MAX),
        Err(DbErr::TryIntoErr {
            from: "u64",
            into: "i32",
            ..
        })
    ));
    assert!(matches!(
        u32::try_from_u64(u64::from(u32::MAX) + 1),
        Err(DbErr::TryIntoErr { into: "u32", .. })
    ));
    assert!(matches!(
        i64::try_from_u64(u64::MAX),
        Err(DbErr::TryIntoErr { into: "i64", .. })
    ));

    // `String` converts unconditionally.
    assert_eq!(String::try_from_u64(42).unwrap(), "42");
    assert_eq!(
        String::try_from_u64(u64::MAX).unwrap(),
        u64::MAX.to_string()
    );

    // Every other implementor refuses, naming itself.
    assert!(matches!(
        bool::try_from_u64(1),
        Err(DbErr::ConvertFromU64("bool"))
    ));
    assert!(matches!(
        f32::try_from_u64(1),
        Err(DbErr::ConvertFromU64("f32"))
    ));
    assert!(matches!(
        f64::try_from_u64(1),
        Err(DbErr::ConvertFromU64("f64"))
    ));
    assert!(matches!(
        <Vec<u8>>::try_from_u64(1),
        Err(DbErr::ConvertFromU64(_))
    ));
    assert!(matches!(
        serde_json::Value::try_from_u64(1),
        Err(DbErr::ConvertFromU64(_))
    ));
    assert!(matches!(
        chrono::NaiveDate::try_from_u64(1),
        Err(DbErr::ConvertFromU64(_))
    ));
    assert!(matches!(
        chrono::NaiveTime::try_from_u64(1),
        Err(DbErr::ConvertFromU64(_))
    ));
    assert!(matches!(
        chrono::NaiveDateTime::try_from_u64(1),
        Err(DbErr::ConvertFromU64(_))
    ));
    assert!(matches!(
        <chrono::DateTime<chrono::FixedOffset>>::try_from_u64(1),
        Err(DbErr::ConvertFromU64(_))
    ));
    assert!(matches!(
        <chrono::DateTime<chrono::Utc>>::try_from_u64(1),
        Err(DbErr::ConvertFromU64(_))
    ));
    assert!(matches!(
        <chrono::DateTime<chrono::Local>>::try_from_u64(1),
        Err(DbErr::ConvertFromU64(_))
    ));
    assert!(matches!(
        rust_decimal::Decimal::try_from_u64(1),
        Err(DbErr::ConvertFromU64(_))
    ));
    assert!(matches!(
        uuid::Uuid::try_from_u64(1),
        Err(DbErr::ConvertFromU64(_))
    ));

    // Tuples refuse at every supported arity.
    assert!(matches!(
        <(i32, i32)>::try_from_u64(1),
        Err(DbErr::ConvertFromU64(_))
    ));
    assert!(matches!(
        <(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32)>::try_from_u64(1),
        Err(DbErr::ConvertFromU64(_))
    ));
}
