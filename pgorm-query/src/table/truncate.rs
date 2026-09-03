use crate::{QueryBuilder, types::*};

/// Truncate a table
///
/// The table is taken by the constructor: `TRUNCATE TABLE` has no spelling
/// without one, so a statement that never names its target does not construct.
///
/// ```compile_fail,E0061
/// use pgorm_query::{tests_cfg::*, *};
///
/// Table::truncate().to_string();
/// ```
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let table = Table::truncate(Font::Table);
///
/// assert_eq!(table.to_string(), r#"TRUNCATE TABLE "font""#);
/// ```
// [spec:pgorm:req:sql.ddl.drop-rename-truncate+3]
#[derive(Debug, Clone)]
pub struct TableTruncateStatement {
    pub(crate) table: TableName,
}

impl TableTruncateStatement {
    /// Construct truncate table statement over the table it empties
    pub fn new<T>(table: T) -> Self
    where
        T: IntoTableName,
    {
        Self {
            table: table.into_table_name(),
        }
    }
}

// [spec:pgorm:req:sql.ddl+5] (the one rendering a DDL statement has)
impl std::fmt::Display for TableTruncateStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_table_truncate_statement(self, &mut sql);
        f.write_str(&sql)
    }
}
