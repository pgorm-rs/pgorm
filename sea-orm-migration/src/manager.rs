// use sea_orm::{
//     sea_query::{
//         extension::postgres::{TypeAlterStatement, TypeCreateStatement, TypeDropStatement}, ForeignKeyCreateStatement, ForeignKeyDropStatement, IndexCreateStatement, IndexDropStatement, PostgresQueryBuilder, TableAlterStatement, TableCreateStatement, TableDropStatement, TableRenameStatement, TableTruncateStatement
//     },
//     DatabasePool, DatabaseTransaction,
// };
// use sea_orm::{ConnectionTrait, DbBackend, DbErr, StatementBuilder};
// use sea_schema::probe::SchemaProbe;

// /// Helper struct for writing migration scripts in migration file
// pub struct SchemaManager<'a> {
//     tx: &'a DatabaseTransaction,
//     conn: DatabasePool,
// }

// impl<'a> SchemaManager<'a> {
//     pub fn new(conn: DatabasePool, tx: &'a DatabaseTransaction) -> Self {
//         Self { conn, tx }
//     }

//     pub async fn exec_stmt<S>(&self, stmt: S) -> Result<(), DbErr>
//     where
//         S: StatementBuilder,
//     {
//         let builder = self.tx.get_database_backend();
//         self.tx.execute(builder.build(&stmt)).await.map(|_| ())
//     }

//     pub fn database_backend(&self) -> DbBackend {
//         self.tx.get_database_backend()
//     }

//     pub(crate) fn transaction(&self) -> &(impl TransactionTrait + ConnectionTrait) {
//         &self.tx
//     }

//     pub fn new_connection(&self) -> DatabasePool {
//         self.conn.clone()
//     }
// }

// /// Schema Creation
// impl SchemaManager<'_> {
//     pub async fn create_table(&self, stmt: TableCreateStatement) -> Result<(), DbErr> {
//         self.exec_stmt(stmt).await
//     }

//     pub async fn create_index(&self, stmt: IndexCreateStatement) -> Result<(), DbErr> {
//         self.exec_stmt(stmt).await
//     }

//     pub async fn create_foreign_key(&self, stmt: ForeignKeyCreateStatement) -> Result<(), DbErr> {
//         self.exec_stmt(stmt).await
//     }

//     pub async fn create_type(&self, stmt: TypeCreateStatement) -> Result<(), DbErr> {
//         self.exec_stmt(stmt).await
//     }
// }

// /// Schema Mutation
// impl SchemaManager<'_> {
//     pub async fn alter_table(&self, stmt: TableAlterStatement) -> Result<(), DbErr> {
//         self.exec_stmt(stmt).await
//     }

//     pub async fn drop_table(&self, stmt: TableDropStatement) -> Result<(), DbErr> {
//         self.exec_stmt(stmt).await
//     }

//     pub async fn rename_table(&self, stmt: TableRenameStatement) -> Result<(), DbErr> {
//         self.exec_stmt(stmt).await
//     }

//     pub async fn truncate_table(&self, stmt: TableTruncateStatement) -> Result<(), DbErr> {
//         self.exec_stmt(stmt).await
//     }

//     pub async fn drop_index(&self, stmt: IndexDropStatement) -> Result<(), DbErr> {
//         self.exec_stmt(stmt).await
//     }

//     pub async fn drop_foreign_key(&self, stmt: ForeignKeyDropStatement) -> Result<(), DbErr> {
//         self.exec_stmt(stmt).await
//     }

//     pub async fn alter_type(&self, stmt: TypeAlterStatement) -> Result<(), DbErr> {
//         self.exec_stmt(stmt).await
//     }

//     pub async fn drop_type(&self, stmt: TypeDropStatement) -> Result<(), DbErr> {
//         self.exec_stmt(stmt).await
//     }
// }

// /// Schema Inspection.
// impl SchemaManager<'_> {
//     pub async fn has_table<T>(&self, table: T) -> Result<bool, DbErr>
//     where
//         T: AsRef<str>,
//     {
//         has_table(&self.tx, table).await
//     }

//     pub async fn has_column<T, C>(&self, table: T, column: C) -> Result<bool, DbErr>
//     where
//         T: AsRef<str>,
//         C: AsRef<str>,
//     {
//         let stmt = sea_schema::postgres::Postgres.has_column(table, column);
//         let (stmt, values) = stmt.build(PostgresQueryBuilder);

//         let res = self
//             .tx
//             .query_one(&stmt, &values)
//             .await?;

//         res.try_get("", "has_column")
//     }

//     pub async fn has_index<T, I>(&self, table: T, index: I) -> Result<bool, DbErr>
//     where
//         T: AsRef<str>,
//         I: AsRef<str>,
//     {
//         let stmt = sea_schema::postgres::Postgres.has_index(table, index);

//         let res = self
//             .tx
//             .query_one(stmt.build(PostgresQueryBuilder))
//             .await?
//             .ok_or_else(|| DbErr::Custom("Failed to check index exists".to_owned()))?;

//         res.try_get("", "has_index")
//     }
// }

// pub(crate) async fn has_table<T>(conn: &(impl TransactionTrait + ConnectionTrait), table: T) -> Result<bool, DbErr>
// where
//     T: AsRef<str>,
// {
//     let stmt = sea_schema::postgres::Postgres.has_table(table);

//     let builder = conn.get_database_backend();
//     let res = conn
//         .query_one(builder.build(&stmt))
//         .await?
//         .ok_or_else(|| DbErr::Custom("Failed to check table exists".to_owned()))?;

//     res.try_get("", "has_table")
// }
