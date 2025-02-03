use inherent::inherent;

use crate::{types::*, QueryBuilder, SchemaStatementBuilder};

/// Drop a table
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let table = Table::truncate().table(Font::Table).to_owned();
///
/// assert_eq!(
///     table.to_string(QueryBuilder),
///     r#"TRUNCATE TABLE "font""#
/// );
/// ```
#[derive(Default, Debug, Clone)]
pub struct TableTruncateStatement {
    pub(crate) table: Option<TableRef>,
}

impl TableTruncateStatement {
    /// Construct truncate table statement
    pub fn new() -> Self {
        Self::default()
    }

    /// Set table name
    pub fn table<T>(&mut self, table: T) -> &mut Self
    where
        T: IntoTableRef,
    {
        self.table = Some(table.into_table_ref());
        self
    }

    pub fn take(&mut self) -> Self {
        Self {
            table: self.table.take(),
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
