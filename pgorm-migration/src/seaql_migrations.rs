use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
// One should override the name of migration table via `MigratorTrait::migration_table_name` method
#[pgorm(table_name = "seaql_migrations")]
pub struct Model {
    #[pgorm(primary_key, auto_increment = false)]
    pub version: String,
    pub applied_at: i64,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
