use inherent::inherent;

use crate::{QueryBuilder, SchemaStatementBuilder, types::*};

/// Rename a table
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let table = Table::rename()
///     .table(Font::Table, Alias::new("font_new"))
///     .to_owned();
///
/// assert_eq!(
///     table.to_string(QueryBuilder),
///     r#"ALTER TABLE "font" RENAME TO "font_new""#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.drop-rename-truncate+2]
#[derive(Default, Debug, Clone)]
pub struct TableRenameStatement {
    pub(crate) from_name: Option<TableName>,
    pub(crate) to_name: Option<DynIden>,
}

impl TableRenameStatement {
    /// Construct rename table statement
    pub fn new() -> Self {
        Self::default()
    }

    /// Set old and new table name.
    ///
    /// The new name is a bare identifier: `RENAME TO` leaves the table in the
    /// schema it is already in, so a qualified target does not construct.
    pub fn table<T, R>(&mut self, from_name: T, to_name: R) -> &mut Self
    where
        T: IntoTableName,
        R: IntoIden,
    {
        self.from_name = Some(from_name.into_table_name());
        self.to_name = Some(to_name.into_iden());
        self
    }

    pub fn take(&mut self) -> Self {
        Self {
            from_name: self.from_name.take(),
            to_name: self.to_name.take(),
        }
    }
}

#[inherent]
impl SchemaStatementBuilder for TableRenameStatement {
    pub fn build(&self, schema_builder: QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_table_rename_statement(self, &mut sql);
        sql
    }

    pub fn build_any(&self, schema_builder: &QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_table_rename_statement(self, &mut sql);
        sql
    }

    pub fn to_string(&self, schema_builder: QueryBuilder) -> String;
}

/// Rename a column of an existing table
///
/// PostgreSQL admits `RENAME` only as the sole action of an `ALTER TABLE`, so a
/// column rename is a statement of its own rather than an option that could be
/// listed beside `ADD COLUMN` or `DROP COLUMN`.
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let table = Table::rename_column()
///     .table(Font::Table)
///     .column(Alias::new("new_col"), Alias::new("new_column"))
///     .to_owned();
///
/// assert_eq!(
///     table.to_string(QueryBuilder),
///     r#"ALTER TABLE "font" RENAME COLUMN "new_col" TO "new_column""#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.alter-table+2]
#[derive(Default, Debug, Clone)]
pub struct ColumnRenameStatement {
    pub(crate) table: Option<TableName>,
    pub(crate) from_name: Option<DynIden>,
    pub(crate) to_name: Option<DynIden>,
}

impl ColumnRenameStatement {
    /// Construct rename column statement
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the table the column belongs to
    pub fn table<T>(&mut self, table: T) -> &mut Self
    where
        T: IntoTableName,
    {
        self.table = Some(table.into_table_name());
        self
    }

    /// Set old and new column name
    pub fn column<T, R>(&mut self, from_name: T, to_name: R) -> &mut Self
    where
        T: IntoIden,
        R: IntoIden,
    {
        self.from_name = Some(from_name.into_iden());
        self.to_name = Some(to_name.into_iden());
        self
    }

    pub fn take(&mut self) -> Self {
        Self {
            table: self.table.take(),
            from_name: self.from_name.take(),
            to_name: self.to_name.take(),
        }
    }
}

#[inherent]
impl SchemaStatementBuilder for ColumnRenameStatement {
    pub fn build(&self, schema_builder: QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_column_rename_statement(self, &mut sql);
        sql
    }

    pub fn build_any(&self, schema_builder: &QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_column_rename_statement(self, &mut sql);
        sql
    }

    pub fn to_string(&self, schema_builder: QueryBuilder) -> String;
}
