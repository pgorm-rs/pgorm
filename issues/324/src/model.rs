use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[pgorm(table_name = "model")]
pub struct Model {
    #[pgorm(primary_key)]
    pub id: AccountId,
    pub name: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Clone, Debug, PartialEq)]
pub struct AccountId(Uuid);

impl From<AccountId> for Uuid {
    fn from(account_id: AccountId) -> Self {
        account_id.0
    }
}

macro_rules! impl_try_from_u64_err {
    ($newtype: ident) => {
        impl pgorm::TryFromU64 for $newtype {
            fn try_from_u64(_n: u64) -> Result<Self, pgorm::DbErr> {
                Err(pgorm::DbErr::ConvertFromU64(stringify!($newtype)))
            }
        }
    };
}

macro_rules! into_pgorm_query_value {
    ($newtype: ident: Box($name: ident)) => {
        impl From<$newtype> for pgorm::Value {
            fn from(source: $newtype) -> Self {
                pgorm::Value::$name(Some(Box::new(source.into())))
            }
        }

        impl pgorm::TryGetable for $newtype {
            fn try_get(
                res: &pgorm::QueryResult,
                pre: &str,
                col: &str,
            ) -> Result<Self, pgorm::TryGetError> {
                let val: $name = res.try_get(pre, col).map_err(pgorm::TryGetError::DbErr)?;
                Ok($newtype(val))
            }
        }

        impl pgorm::pgorm_query::Nullable for $newtype {
            fn null() -> pgorm::Value {
                pgorm::Value::$name(None)
            }
        }

        impl pgorm::pgorm_query::ValueType for $newtype {
            fn try_from(v: pgorm::Value) -> Result<Self, pgorm::pgorm_query::ValueTypeErr> {
                match v {
                    pgorm::Value::$name(Some(x)) => Ok($newtype(*x)),
                    _ => Err(pgorm::pgorm_query::ValueTypeErr),
                }
            }

            fn type_name() -> String {
                stringify!($newtype).to_owned()
            }

            fn array_type() -> pgorm::pgorm_query::ArrayType {
                pgorm::pgorm_query::ArrayType::$name
            }

            fn column_type() -> pgorm::pgorm_query::ColumnType {
                pgorm::pgorm_query::ColumnType::$name
            }
        }
    };
}

into_pgorm_query_value!(AccountId: Box(Uuid));
impl_try_from_u64_err!(AccountId);
