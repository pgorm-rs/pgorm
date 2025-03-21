use inherent::inherent;

use crate::{QueryBuilder, SchemaStatementBuilder, TableForeignKey, types::*};

/// Drop a foreign key constraint for an existing table
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let foreign_key = ForeignKey::drop()
///     .name("FK_character_id")
///     .table(Character::Table)
///     .to_owned();
///
/// assert_eq!(
///     foreign_key.to_string(QueryBuilder),
///     r#"ALTER TABLE "character" DROP CONSTRAINT "FK_character_id""#
/// );
/// ```
#[derive(Default, Debug, Clone)]
pub struct ForeignKeyDropStatement {
    pub(crate) foreign_key: TableForeignKey,
    pub(crate) table: Option<TableRef>,
}

impl ForeignKeyDropStatement {
    /// Construct a new [`ForeignKeyDropStatement`]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set foreign key name
    pub fn name<T>(&mut self, name: T) -> &mut Self
    where
        T: Into<String>,
    {
        self.foreign_key.name(name);
        self
    }

    /// Set key table and referencing table
    pub fn table<T>(&mut self, table: T) -> &mut Self
    where
        T: IntoTableRef,
    {
        self.table = Some(table.into_table_ref());
        self
    }
}

#[inherent]
impl SchemaStatementBuilder for ForeignKeyDropStatement {
    pub fn build(&self, schema_builder: QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_foreign_key_drop_statement(self, &mut sql);
        sql
    }

    pub fn build_any(&self, schema_builder: &QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        schema_builder.prepare_foreign_key_drop_statement(self, &mut sql);
        sql
    }

    pub fn to_string(&self, schema_builder: QueryBuilder) -> String;
}
