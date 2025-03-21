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
