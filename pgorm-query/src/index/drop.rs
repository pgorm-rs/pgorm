use inherent::inherent;

use crate::{types::*, QueryBuilder, SchemaStatementBuilder, TableIndex};

/// Drop an index for an existing table
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::drop()
///     .name("idx-character-id")
///     .table(Character::Table)
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(QueryBuilder),
///     r#"DROP INDEX "idx-character-id""#
/// );
/// ```
#[derive(Default, Debug, Clone)]
pub struct IndexDropStatement {
    pub(crate) table: Option<TableRef>,
    pub(crate) index: TableIndex,
    pub(crate) if_exists: bool,
}

impl IndexDropStatement {
    /// Construct a new [`IndexDropStatement`]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set index name
    pub fn name<T>(&mut self, name: T) -> &mut Self
    where
        T: Into<String>,
    {
        self.index.name(name);
        self
    }

    /// Set target table
    pub fn table<T>(&mut self, table: T) -> &mut Self
    where
        T: IntoTableRef,
    {
        self.table = Some(table.into_table_ref());
        self
    }

    pub fn if_exists(&mut self) -> &mut Self {
        self.if_exists = true;
        self
    }
}

#[inherent]
impl SchemaStatementBuilder for IndexDropStatement {
    pub fn build(&self, schema_builder: QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_index_drop_statement(self, &mut sql);
        sql
    }

    pub fn build_any(&self, schema_builder: &QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_index_drop_statement(self, &mut sql);
        sql
    }

    pub fn to_string(&self, schema_builder: QueryBuilder) -> String;
}
