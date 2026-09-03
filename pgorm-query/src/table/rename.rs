use crate::{QueryBuilder, types::*};

/// Rename a table
///
/// Both names are taken by the constructor: a rename that names neither end has
/// no PostgreSQL spelling, so it does not construct.
///
/// ```compile_fail,E0061
/// use pgorm_query::{tests_cfg::*, *};
///
/// Table::rename().to_string();
/// ```
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let table = Table::rename(Font::Table, Alias::new("font_new"));
///
/// assert_eq!(
///     table.to_string(),
///     r#"ALTER TABLE "font" RENAME TO "font_new""#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.drop-rename-truncate+3]
#[derive(Debug, Clone)]
pub struct TableRenameStatement {
    pub(crate) from_name: TableName,
    pub(crate) to_name: DynIden,
}

impl TableRenameStatement {
    /// Construct rename table statement from the old and new table name.
    ///
    /// The new name is a bare identifier: `RENAME TO` leaves the table in the
    /// schema it is already in, so a qualified target does not construct.
    pub fn new<T, R>(from_name: T, to_name: R) -> Self
    where
        T: IntoTableName,
        R: IntoIden,
    {
        Self {
            from_name: from_name.into_table_name(),
            to_name: to_name.into_iden(),
        }
    }
}

// [spec:pgorm:req:sql.ddl+5] (the one rendering a DDL statement has)
impl std::fmt::Display for TableRenameStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_table_rename_statement(self, &mut sql);
        f.write_str(&sql)
    }
}

/// Rename a column of an existing table
///
/// PostgreSQL admits `RENAME` only as the sole action of an `ALTER TABLE`, so a
/// column rename is a statement of its own rather than an option that could be
/// listed beside `ADD COLUMN` or `DROP COLUMN`.
///
/// All three names are taken by the constructor: none of them has a spelling the
/// grammar can do without, so a partly-named rename does not construct.
///
/// ```compile_fail,E0061
/// use pgorm_query::{tests_cfg::*, *};
///
/// Table::rename_column().to_string();
/// ```
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let table = Table::rename_column(
///     Font::Table,
///     Alias::new("new_col"),
///     Alias::new("new_column"),
/// );
///
/// assert_eq!(
///     table.to_string(),
///     r#"ALTER TABLE "font" RENAME COLUMN "new_col" TO "new_column""#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.alter-table+3]
#[derive(Debug, Clone)]
pub struct ColumnRenameStatement {
    pub(crate) table: TableName,
    pub(crate) from_name: DynIden,
    pub(crate) to_name: DynIden,
}

impl ColumnRenameStatement {
    /// Construct rename column statement from the table and the two column names
    pub fn new<T, F, R>(table: T, from_name: F, to_name: R) -> Self
    where
        T: IntoTableName,
        F: IntoIden,
        R: IntoIden,
    {
        Self {
            table: table.into_table_name(),
            from_name: from_name.into_iden(),
            to_name: to_name.into_iden(),
        }
    }
}

// [spec:pgorm:req:sql.ddl+5] (the one rendering a DDL statement has)
impl std::fmt::Display for ColumnRenameStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_column_rename_statement(self, &mut sql);
        f.write_str(&sql)
    }
}
