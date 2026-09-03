use crate::{QueryBuilder, types::*};

/// Drop an index for an existing table
///
/// The index name is taken by the constructor, being the whole of what
/// `DROP INDEX` names. The table is optional and contributes only its schema:
/// PostgreSQL scopes indexes to a schema rather than to a table, and accepts a
/// bare `DROP INDEX "name"`.
///
/// ```compile_fail,E0061
/// use pgorm_query::{*, tests_cfg::*};
///
/// Index::drop().table(Character::Table);
/// ```
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let index = Index::drop("idx-character-id")
///     .table(Character::Table)
///     .to_owned();
///
/// assert_eq!(
///     index.to_string(),
///     r#"DROP INDEX "idx-character-id""#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.index-drop+2]
#[derive(Debug, Clone)]
pub struct IndexDropStatement {
    pub(crate) table: Option<TableName>,
    pub(crate) name: DynIden,
    pub(crate) if_exists: bool,
}

impl IndexDropStatement {
    /// Construct a new [`IndexDropStatement`] over the index it drops
    pub fn new<T>(name: T) -> Self
    where
        T: IntoIden,
    {
        Self {
            table: None,
            name: name.into_iden(),
            if_exists: false,
        }
    }

    /// Set the schema-bearing target table
    pub fn table<T>(&mut self, table: T) -> &mut Self
    where
        T: IntoTableName,
    {
        self.table = Some(table.into_table_name());
        self
    }

    pub fn if_exists(&mut self) -> &mut Self {
        self.if_exists = true;
        self
    }
}

// [spec:pgorm:req:sql.ddl+5] (the one rendering a DDL statement has)
impl std::fmt::Display for IndexDropStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_index_drop_statement(self, &mut sql);
        f.write_str(&sql)
    }
}
