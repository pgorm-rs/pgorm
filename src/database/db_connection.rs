use std::{collections::BTreeMap, future::Future, ops::AsyncFn, pin::Pin, sync::Arc};

use crate::{error::*, ConnectionTrait, TransactionTrait};
use deadpool::Status;
use pgorm_pool::{Object, Pool, Transaction};
use tokio_postgres::{
    types::{BorrowToSql, ToSql},
    ToStatement,
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
    #[deprecated(note = "Dangerous function. Use .transaction")]
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

    pub async fn rollback(mut self) -> Result<(), DbErr> {
        if let Some(tx) = self.0.take() {
            tx.rollback().await.map_err(|e| DbErr::Postgres(e))
        } else {
            unreachable!()
        }
    }
}

// #[async_trait::async_trait]
// impl ConnectionTrait for DatabasePool {
//     async fn execute<T>(&self, statement: &T, params: &[&(dyn ToSql + Sync)]) -> Result<u64, DbErr>
//     where
//         T: ?Sized + ToStatement + Send + Sync,
//     {
//         let conn = self.0.get().await?;
//         Ok(conn.execute(statement, params).await?)
//     }

//     async fn execute_raw<T, P, I>(&self, statement: &T, params: I) -> Result<u64, DbErr>
//     where
//         T: ?Sized + ToStatement + Send + Sync,
//         P: BorrowToSql,
//         I: IntoIterator<Item = P> + Send,
//         I::IntoIter: ExactSizeIterator,
//     {
//         let conn = self.0.get().await?;
//         Ok(conn.execute_raw(statement, params).await?)
//     }

//     async fn query_one<T>(
//         &self,
//         statement: &T,
//         params: &[&(dyn ToSql + Sync)],
//     ) -> Result<tokio_postgres::Row, DbErr>
//     where
//         T: ?Sized + ToStatement + Send + Sync,
//     {
//         let conn = self.0.get().await?;
//         Ok(conn.query_one(statement, params).await?)
//     }

//     async fn query_opt<T>(
//         &self,
//         statement: &T,
//         params: &[&(dyn ToSql + Sync)],
//     ) -> Result<Option<tokio_postgres::Row>, DbErr>
//     where
//         T: ?Sized + ToStatement + Send + Sync,
//     {
//         let conn = self.0.get().await?;
//         Ok(conn.query_opt(statement, params).await?)
//     }

//     async fn query_all<T>(
//         &self,
//         statement: &T,
//         params: &[&(dyn ToSql + Sync)],
//     ) -> Result<Vec<tokio_postgres::Row>, DbErr>
//     where
//         T: ?Sized + ToStatement + Send + Sync,
//     {
//         let conn = self.0.get().await?;
//         Ok(conn.query(statement, params).await?)
//     }
// }

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

impl DatabaseConnection {
    // fn transaction<'a, F: AsyncFn(&mut DatabaseTransaction<'a>) -> Result<(), DbErr>>(
    //     &'a mut self,
    //     closure: F,
    // ) -> impl Future<Output = Result<(), DbErr>> + use<'a, F> {
    //     async move {
    //         let mut transaction =
    //             DatabaseTransaction(Some(self.0.build_transaction().start().await?));

    //         match closure(&mut transaction).await {
    //             Ok(_) => {
    //                 transaction.commit().await?;
    //                 Ok(())
    //             }
    //             Err(e) => {
    //                 transaction.rollback().await?;
    //                 Err(e)
    //             }
    //         }
    //     }
    // }
    // async fn transaction<F: AsyncFn(&mut DatabaseTransaction<'_>) -> Result<(), DbErr>>(
    //     &mut self,
    //     closure: F,
    // ) -> Result<(), DbErr> {
    //     let mut transaction =
    //         DatabaseTransaction(Some(self.0.build_transaction().start().await?));

    //     match closure(&mut transaction).await {
    //         Ok(_) => {
    //             transaction.commit().await?;
    //             Ok(())
    //         }
    //         Err(e) => {
    //             transaction.rollback().await?;
    //             Err(e)
    //         }
    //     }
    // }
}


#[async_trait::async_trait(?Send)]
impl TransactionTrait for DatabaseTransaction<'_> {
    async fn transaction<T, F: AsyncFn(&mut DatabaseTransaction<'_>) -> Result<T, DbErr>>(
        &mut self,
        closure: F,
    ) -> Result<T, DbErr> {
        let mut transaction =
            DatabaseTransaction(Some(self.0.as_mut().unwrap().transaction().await?));

        match closure(&mut transaction).await {
            Ok(x) => {
                transaction.commit().await?;
                Ok(x)
            }
            Err(e) => {
                transaction.rollback().await?;
                Err(e)
            }
        }
    }
}

#[async_trait::async_trait(?Send)]
impl TransactionTrait for DatabaseConnection {
    async fn transaction<T, F: AsyncFn(&mut DatabaseTransaction<'_>) -> Result<T, DbErr>>(
        &mut self,
        closure: F,
    ) -> Result<T, DbErr> {
        let mut transaction =
            DatabaseTransaction(Some(self.0.build_transaction().start().await?));

        match closure(&mut transaction).await {
            Ok(x) => {
                transaction.commit().await?;
                Ok(x)
            }
            Err(e) => {
                transaction.rollback().await?;
                Err(e)
            }
        }
    }
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
