use super::pgorm_active_enums::*;
use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[cfg_attr(feature = "sqlx-postgres", pgorm(schema_name = "public"))]
#[pgorm(table_name = "active_enum")]
pub struct Model {
    #[pgorm(primary_key)]
    pub id: i32,
    pub categories: Option<Vec<Category>>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
