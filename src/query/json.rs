use crate::{error::*, FromQueryResult, QueryResult};
use serde_json::Map;
pub use serde_json::Value as JsonValue;

impl FromQueryResult for JsonValue {
    #[allow(unused_variables, unused_mut)]
    fn from_query_result(res: &QueryResult, pre: &str) -> Result<Self, DbErr> {
        todo!()
        // let mut map = Map::new();
        // #[allow(unused_macros)]
        // macro_rules! try_get_type {
        //     ( $type: ty, $col: ident ) => {
        //         if let Ok(v) = res.try_get::<Option<$type>>(pre, &$col) {
        //             map.insert($col.to_owned(), json!(v));
        //             continue;
        //         }
        //     };
        // }
        // let row = &res.row;

        // use serde_json::json;

        // for column in row.columns() {
        //     let col = if !column.name().starts_with(pre) {
        //         continue;
        //     } else {
        //         column.name().replacen(pre, "", 1)
        //     };
        //     let col_type = column.type_info();

        //     macro_rules! match_postgres_type {
        //         ( $type: ty ) => {
        //             match col_type.kind() {
        //                 #[cfg(feature = "postgres-array")]
        //                 sqlx::postgres::PgTypeKind::Array(_) => {
        //                     if <Vec<$type> as Type<Postgres>>::type_info().eq(col_type) {
        //                         try_get_type!(Vec<$type>, col);
        //                     }
        //                 }
        //                 _ => {
        //                     if <$type as Type<Postgres>>::type_info().eq(col_type) {
        //                         try_get_type!($type, col);
        //                     }
        //                 }
        //             }
        //         };
        //     }

        //     match_postgres_type!(bool);
        //     match_postgres_type!(i8);
        //     match_postgres_type!(i16);
        //     match_postgres_type!(i32);
        //     match_postgres_type!(i64);
        //     // match_postgres_type!(u8); // unsupported by SQLx Postgres
        //     // match_postgres_type!(u16); // unsupported by SQLx Postgres
        //     // Since 0.6.0, SQLx has dropped direct mapping from PostgreSQL's OID to Rust's `u32`;
        //     // Instead, `u32` was wrapped by a `sqlx::Oid`.
        //     if <Oid as Type<Postgres>>::type_info().eq(col_type) {
        //         try_get_type!(u32, col)
        //     }
        //     // match_postgres_type!(u64); // unsupported by SQLx Postgres
        //     match_postgres_type!(f32);
        //     match_postgres_type!(f64);
        //     #[cfg(feature = "with-chrono")]
        //     match_postgres_type!(chrono::NaiveDate);
        //     #[cfg(feature = "with-chrono")]
        //     match_postgres_type!(chrono::NaiveTime);
        //     #[cfg(feature = "with-chrono")]
        //     match_postgres_type!(chrono::NaiveDateTime);
        //     #[cfg(feature = "with-chrono")]
        //     match_postgres_type!(chrono::DateTime<chrono::FixedOffset>);
        //     #[cfg(feature = "with-time")]
        //     match_postgres_type!(time::Date);
        //     #[cfg(feature = "with-time")]
        //     match_postgres_type!(time::Time);
        //     #[cfg(feature = "with-time")]
        //     match_postgres_type!(time::PrimitiveDateTime);
        //     #[cfg(feature = "with-time")]
        //     match_postgres_type!(time::OffsetDateTime);
        //     #[cfg(feature = "with-rust_decimal")]
        //     match_postgres_type!(rust_decimal::Decimal);
        //     #[cfg(feature = "with-json")]
        //     try_get_type!(serde_json::Value, col);
        //     #[cfg(all(feature = "with-json", feature = "postgres-array"))]
        //     try_get_type!(Vec<serde_json::Value>, col);
        //     try_get_type!(String, col);
        //     #[cfg(feature = "postgres-array")]
        //     try_get_type!(Vec<String>, col);
        //     #[cfg(feature = "with-uuid")]
        //     try_get_type!(uuid::Uuid, col);
        //     #[cfg(all(feature = "with-uuid", feature = "postgres-array"))]
        //     try_get_type!(Vec<uuid::Uuid>, col);
        //     try_get_type!(Vec<u8>, col);
        // }
        // Ok(JsonValue::Object(map))
    }
}

#[cfg(test)]
#[cfg(feature = "mock")]
mod tests {
    use crate::tests_cfg::cake;
    use crate::{entity::*, DbBackend, DbErr, MockDatabase};
    use sea_query::Value;

    #[smol_potat::test]
    async fn to_json_1() -> Result<(), DbErr> {
        let db = MockDatabase::new(DbBackend::Postgres)
            .append_query_results([[maplit::btreemap! {
                "id" => Into::<Value>::into(128), "name" => Into::<Value>::into("apple")
            }]])
            .into_connection();

        assert_eq!(
            cake::Entity::find().into_json().one(&db).await.unwrap(),
            Some(serde_json::json!({
                "id": 128,
                "name": "apple"
            }))
        );

        Ok(())
    }
}
