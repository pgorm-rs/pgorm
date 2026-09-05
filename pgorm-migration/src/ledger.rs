use pgorm::entity::prelude::*;

// [spec:pgorm:def:migration.runner+1]    ledger schema and default name
// [spec:pgorm:req:migration.checksum]    the nullable third column
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
// One should override the name of migration table via `MigratorTrait::migration_table_name` method
#[pgorm(table_name = "pgorm_migrations")]
pub struct Model {
    #[pgorm(primary_key, auto_increment = false)]
    pub version: String,
    pub applied_at: i64,
    /// The migration's `checksum()` at the time it was applied, or `NULL` for a
    /// row written before the column existed or by a migration that does not
    /// report one.
    pub checksum: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
