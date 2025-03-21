use std::{collections::BTreeMap, sync::Arc};

use crate::{ConnectionTrait, TransactionTrait, error::*};
use deadpool::Status;
use pgorm_pool::{Object, Pool, Transaction};
use tokio_postgres::{
    ToStatement,
    types::{BorrowToSql, ToSql},
};

/// Handle a database connection depending on the backend enabled by the feature
/// flags. This creates a database pool.
#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct DatabasePool(pub(crate) Pool);

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct DatabaseMultiPool(pub(crate) BTreeMap<Arc<String>, DatabasePool>);

impl DatabaseMultiPool {
    pub fn get(&self, key: Arc<String>) -> Option<DatabasePool> {
        self.0.get(&key).cloned()
    }

    pub fn status(&self) -> BTreeMap<Arc<String>, Status> {
        self.0
            .iter()
            .map(|(k, v)| (k.clone(), v.status()))
            .collect()
    }
}

// impl Deref for DatabasePool {
//     type Target = Pool;

//     fn deref(&self) -> &Self::Target {
//         &self.0
//     }
// }

impl DatabasePool {
    pub async fn get(&self) -> Result<DatabaseConnection, DbErr> {
        let conn = Pool::get(&self.0).await?;
        Ok(DatabaseConnection(conn))
    }

    pub fn tag(&self) -> Arc<String> {
        self.0.manager().tag()
    }

    pub fn status(&self) -> Status {
        self.0.status()
    }
}

#[derive(Debug)]
pub struct DatabaseConnection(pub(crate) Object);

impl DatabaseConnection {
    async fn begin_with_config(
        &mut self,
        read_only: bool,
        isolation_level: Option<tokio_postgres::IsolationLevel>,
    ) -> Result<DatabaseTransaction<'_>, DbErr> {
        let mut t = self.0.build_transaction();

        if let Some(l) = isolation_level {
            t = t.isolation_level(l);
        }

        if read_only {
            t = t.read_only(true);
        }

        Ok(DatabaseTransaction(Some(t.start().await?)))
    }
}

#[derive(Debug)]
pub struct DatabaseTransaction<'a>(pub(crate) Option<Transaction<'a>>);

impl DatabaseTransaction<'_> {
    pub async fn commit(mut self) -> Result<(), DbErr> {
        if let Some(tx) = self.0.take() {
            tx.commit().await.map_err(|e| DbErr::Postgres(e))
        } else {
            unreachable!()
        }
    }
}

impl Drop for DatabaseTransaction<'_> {
    fn drop(&mut self) {
        if self.0.is_some() {
            tracing::warn!("Transaction dropped without committing!");
        }
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for &DatabaseConnection {
    // #[instrument(level = "trace")]
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.execute(statement, params).await?)
    }

    // #[instrument(level = "trace")]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(self.0.execute_raw(statement, params).await?)
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<tokio_postgres::Row, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.query_one(statement, params).await?)
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.query_opt(statement, params).await?)
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.query(statement, params).await?)
    }
}

#[async_trait::async_trait]
impl ConnectionTrait for DatabaseConnection {
    // #[instrument(level = "trace")]
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.execute(statement, params).await?)
    }

    // #[instrument(level = "trace")]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(self.0.execute_raw(statement, params).await?)
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<tokio_postgres::Row, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.query_one(statement, params).await?)
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.query_opt(statement, params).await?)
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.query(statement, params).await?)
    }

    // async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, DbErr>
    // where
    //     T: ?Sized + ToStatement,
    //     P: BorrowToSql,
    //     I: IntoIterator<Item = P> + Send,
    //     I::IntoIter: ExactSizeIterator
    // {
    //     Ok(self.0.query_raw(statement, params).await?)
    // }
}

#[async_trait::async_trait]
impl ConnectionTrait for DatabaseTransaction<'_> {
    // #[instrument(level = "trace")]
    async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.as_ref().unwrap().execute(statement, params).await?)
    }

    // #[instrument(level = "trace")]
    async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
        P: BorrowToSql,
        I: IntoIterator<Item = P> + Send,
        I::IntoIter: ExactSizeIterator,
    {
        Ok(self
            .0
            .as_ref()
            .unwrap()
            .execute_raw(statement, params)
            .await?)
    }

    async fn query_one<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<tokio_postgres::Row, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self
            .0
            .as_ref()
            .unwrap()
            .query_one(statement, params)
            .await?)
    }

    async fn query_opt<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Option<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self
            .0
            .as_ref()
            .unwrap()
            .query_opt(statement, params)
            .await?)
    }

    async fn query_all<T>(
        &self,
        statement: &T,
        params: &[&(dyn ToSql + Sync)],
    ) -> Result<Vec<tokio_postgres::Row>, DbErr>
    where
        T: ?Sized + ToStatement + Send + Sync,
    {
        Ok(self.0.as_ref().unwrap().query(statement, params).await?)
    }

    // async fn query_raw<T, P, I>(&self, statement: &T, params: I) -> Result<RowStream, DbErr>
    // where
    //     T: ?Sized + ToStatement,
    //     P: BorrowToSql,
    //     I: IntoIterator<Item = P>,
    //     I::IntoIter: ExactSizeIterator
    // {
    //     Ok(self.query_raw(statement, params).await?)
    // }
}
#[async_trait::async_trait]
impl TransactionTrait for DatabaseTransaction<'_> {
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, DbErr> {
        Ok(DatabaseTransaction(Some(
            self.0.as_mut().unwrap().transaction().await?,
        )))
    }
}

#[async_trait::async_trait]
impl TransactionTrait for DatabaseConnection {
    async fn begin(&mut self) -> Result<DatabaseTransaction<'_>, DbErr> {
        let tx = self.0.transaction().await?;
        Ok(DatabaseTransaction(Some(tx)))
    }
    // #[instrument(level = "trace")]
    // async fn begin(&self) -> Result<DatabaseTransaction, DbErr> {
    //     let conn = self.0.get().await?;
    //     conn.transaction()
    //     match self.0.as_ref() {
    //         #[cfg(feature = "sqlx-postgres")]
    //         Some(conn) => conn.begin(None, None).await,
    //         None => Err(conn_err("Disconnected")),
    //     }
    // }

    // #[instrument(level = "trace")]
    // async fn begin_with_config(
    //     &self,
    //     isolation_level: Option<IsolationLevel>,
    //     access_mode: Option<AccessMode>,
    // ) -> Result<DatabaseTransaction, DbErr> {
    //     match self.0.as_ref() {
    //         #[cfg(feature = "sqlx-postgres")]
    //         Some(conn) => conn.begin(isolation_level, access_mode).await,
    //         None => Err(conn_err("Disconnected")),
    //     }
    // }

    // /// Execute the function inside a transaction.
    // /// If the function returns an error, the transaction will be rolled back. If it does not return an error, the transaction will be committed.
    // #[instrument(level = "trace", skip(callback))]
    // async fn transaction<F, T, E>(&self, callback: F) -> Result<T, TransactionError<E>>
    // where
    //     F: for<'c> FnOnce(
    //             &'c DatabaseTransaction,
    //         ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'c>>
    //         + Send,
    //     T: Send,
    //     E: std::error::Error + Send,
    // {
    //     match self.0.as_ref() {
    //         #[cfg(feature = "sqlx-postgres")]
    //         Some(conn) => conn.transaction(callback, None, None).await,

    //         None => Err(conn_err("Disconnected").into()),
    //     }
    // }

    // /// Execute the function inside a transaction.
    // /// If the function returns an error, the transaction will be rolled back. If it does not return an error, the transaction will be committed.
    // #[instrument(level = "trace", skip(callback))]
    // async fn transaction_with_config<F, T, E>(
    //     &self,
    //     callback: F,
    //     isolation_level: Option<IsolationLevel>,
    //     access_mode: Option<AccessMode>,
    // ) -> Result<T, TransactionError<E>>
    // where
    //     F: for<'c> FnOnce(
    //             &'c DatabaseTransaction,
    //         ) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'c>>
    //         + Send,
    //     T: Send,
    //     E: std::error::Error + Send,
    // {
    //     match self.0.as_ref() {
    //         #[cfg(feature = "sqlx-postgres")]
    //         Some(conn) => {
    //             conn.transaction(callback, isolation_level, access_mode)
    //                 .await
    //         }
    //         None => Err(conn_err("Disconnected").into()),
    //     }
    // }
}
#[cfg(test)]
mod tests {
    use crate::DatabasePool;

    #[test]
    fn assert_database_connection_traits() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DatabasePool>();
    }
}
