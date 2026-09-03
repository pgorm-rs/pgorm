use crate::{QueryBuilder, types::*};

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
///     foreign_key.to_string(),
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

// [spec:pgorm:req:sql.ddl+5] (the one rendering a DDL statement has)
impl std::fmt::Display for ForeignKeyDropStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_foreign_key_drop_statement(self, &mut sql);
        f.write_str(&sql)
    }
}
