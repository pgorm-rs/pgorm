use inherent::inherent;

use crate::{QueryBuilder, SchemaStatementBuilder, types::*};

/// Drop a table
///
/// `DROP TABLE` names at least one table, so the first one is taken by the
/// constructor and `table()` appends the rest: the empty target list PostgreSQL
/// rejects has nowhere to come from.
///
/// ```compile_fail,E0061
/// use pgorm_query::{tests_cfg::*, *};
///
/// Table::drop().if_exists();
/// ```
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let table = Table::drop(Glyph::Table).table(Char::Table).to_owned();
///
/// assert_eq!(
///     table.to_string(QueryBuilder),
///     r#"DROP TABLE "glyph", "character""#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.drop-rename-truncate+3]
#[derive(Debug, Clone)]
pub struct TableDropStatement {
    pub(crate) tables: Vec<TableName>,
    pub(crate) options: Vec<TableDropOpt>,
    pub(crate) if_exists: bool,
}

/// All available table drop options
#[derive(Debug, Clone)]
pub enum TableDropOpt {
    Restrict,
    Cascade,
}

impl TableDropStatement {
    /// Construct drop table statement over its first table
    pub fn new<T>(table: T) -> Self
    where
        T: IntoTableName,
    {
        Self {
            tables: vec![table.into_table_name()],
            options: Vec::new(),
            if_exists: false,
        }
    }

    /// Add a further table to drop, after the one the constructor took
    pub fn table<T>(&mut self, table: T) -> &mut Self
    where
        T: IntoTableName,
    {
        self.tables.push(table.into_table_name());
        self
    }

    /// Drop table if exists
    pub fn if_exists(&mut self) -> &mut Self {
        self.if_exists = true;
        self
    }

    /// Drop option restrict
    pub fn restrict(&mut self) -> &mut Self {
        self.options.push(TableDropOpt::Restrict);
        self
    }

    /// Drop option cacade
    pub fn cascade(&mut self) -> &mut Self {
        self.options.push(TableDropOpt::Cascade);
        self
    }

    /// Clone this statement out of a builder chain.
    ///
    /// The tables are copied rather than moved: moving them out would leave the
    /// targetless statement this type exists to rule out.
    pub fn take(&mut self) -> Self {
        Self {
            tables: self.tables.clone(),
            options: std::mem::take(&mut self.options),
            if_exists: self.if_exists,
        }
    }
}

#[inherent]
impl SchemaStatementBuilder for TableDropStatement {
    pub fn build(&self, schema_builder: QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_table_drop_statement(self, &mut sql);
        sql
    }

    pub fn build_any(&self, schema_builder: &QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_table_drop_statement(self, &mut sql);
        sql
    }

    pub fn to_string(&self, schema_builder: QueryBuilder) -> String;
}
