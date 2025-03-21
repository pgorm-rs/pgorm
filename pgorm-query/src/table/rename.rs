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
#[derive(Default, Debug, Clone)]
pub struct TableRenameStatement {
    pub(crate) from_name: Option<TableRef>,
    pub(crate) to_name: Option<TableRef>,
}

impl TableRenameStatement {
    /// Construct rename table statement
    pub fn new() -> Self {
        Self::default()
    }

    /// Set old and new table name
    pub fn table<T, R>(&mut self, from_name: T, to_name: R) -> &mut Self
    where
        T: IntoTableRef,
        R: IntoTableRef,
    {
        self.from_name = Some(from_name.into_table_ref());
        self.to_name = Some(to_name.into_table_ref());
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
