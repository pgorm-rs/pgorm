use pgorm::entity::prelude::*;
use std::marker::PhantomData;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[pgorm(table_name = "model")]
pub struct Model {
    #[pgorm(primary_key)]
    pub id: AccountId<String>,
    pub name: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

#[derive(Clone, Debug, PartialEq)]
pub struct AccountId<T>(Uuid, PhantomData<T>);

impl<T> AccountId<T> {
    pub fn new(id: Uuid) -> Self {
        AccountId(id, PhantomData)
    }
}

impl<T> From<AccountId<T>> for Uuid {
    fn from(account_id: AccountId<T>) -> Self {
        account_id.0
    }
}

impl<T> pgorm::TryFromU64 for AccountId<T> {
    fn try_from_u64(_n: u64) -> Result<Self, pgorm::DbErr> {
        Err(pgorm::DbErr::ConvertFromU64(stringify!(AccountId<T>)))
    }
}

impl<T> From<AccountId<T>> for pgorm::Value {
    fn from(source: AccountId<T>) -> Self {
        pgorm::Value::Uuid(Some(Box::new(source.into())))
    }
}

impl<T> pgorm::TryGetable for AccountId<T> {
    fn try_get(
        res: &pgorm::QueryResult,
        pre: &str,
        col: &str,
    ) -> Result<Self, pgorm::TryGetError> {
        let val: Uuid = res.try_get(pre, col).map_err(pgorm::TryGetError::DbErr)?;
        Ok(AccountId::<T>::new(val))
    }
}

impl<T> pgorm::pgorm_query::Nullable for AccountId<T> {
    fn null() -> pgorm::Value {
        pgorm::Value::Uuid(None)
    }
}

impl<T> pgorm::pgorm_query::ValueType for AccountId<T> {
    fn try_from(v: pgorm::Value) -> Result<Self, pgorm::pgorm_query::ValueTypeErr> {
        match v {
            pgorm::Value::Uuid(Some(x)) => Ok(AccountId::<T>::new(*x)),
            _ => Err(pgorm::pgorm_query::ValueTypeErr),
        }
    }

    fn type_name() -> String {
        stringify!(AccountId).to_owned()
    }

    fn array_type() -> pgorm::pgorm_query::ArrayType {
        pgorm::pgorm_query::ArrayType::Uuid
    }

    fn column_type() -> pgorm::pgorm_query::ColumnType {
        pgorm::pgorm_query::ColumnType::Uuid
    }
}
