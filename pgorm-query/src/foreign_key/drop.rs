use inherent::inherent;

use crate::{QueryBuilder, SchemaStatementBuilder, types::*};

/// Drop a foreign key constraint for an existing table
///
/// Both the table and the constraint name are taken by the constructor:
/// `ALTER TABLE ... DROP CONSTRAINT` renders both and PostgreSQL rejects the
/// statement without either, so a partly-named drop does not construct.
///
/// ```compile_fail,E0061
/// use pgorm_query::{*, tests_cfg::*};
///
/// ForeignKey::drop().name("FK_character_id");
/// ```
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// let foreign_key = ForeignKey::drop(Character::Table, "FK_character_id");
///
/// assert_eq!(
///     foreign_key.to_string(QueryBuilder),
///     r#"ALTER TABLE "character" DROP CONSTRAINT "FK_character_id""#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.foreign-key+2]
#[derive(Debug, Clone)]
pub struct ForeignKeyDropStatement {
    pub(crate) name: DynIden,
    pub(crate) table: TableName,
}

impl ForeignKeyDropStatement {
    /// Construct a new [`ForeignKeyDropStatement`] over its table and constraint
    pub fn new<T, N>(table: T, name: N) -> Self
    where
        T: IntoTableName,
        N: IntoIden,
    {
        Self {
            name: name.into_iden(),
            table: table.into_table_name(),
        }
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
