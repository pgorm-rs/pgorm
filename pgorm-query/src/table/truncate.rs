use inherent::inherent;

use crate::{QueryBuilder, SchemaStatementBuilder, types::*};

/// Truncate a table
///
/// The table is taken by the constructor: `TRUNCATE TABLE` has no spelling
/// without one, so a statement that never names its target does not construct.
///
/// ```compile_fail,E0061
/// use pgorm_query::{tests_cfg::*, *};
///
/// Table::truncate().to_string(QueryBuilder);
/// ```
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let table = Table::truncate(Font::Table);
///
/// assert_eq!(table.to_string(QueryBuilder), r#"TRUNCATE TABLE "font""#);
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

#[inherent]
impl SchemaStatementBuilder for TableTruncateStatement {
    pub fn build(&self, schema_builder: QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_table_truncate_statement(self, &mut sql);
        sql
    }

    pub fn build_any(&self, schema_builder: &QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_table_truncate_statement(self, &mut sql);
        sql
    }

    pub fn to_string(&self, schema_builder: QueryBuilder) -> String;
}
