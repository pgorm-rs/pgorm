use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[pgorm(table_name = "model")]
pub struct Model {
    #[pgorm(primary_key, auto_increment = false)]
    pub id_1: i32,
    pub owner: String,
    pub name: String,
    pub description: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
