use pgorm::entity::prelude::*;

use super::pgorm_range_types::Floatrange;
use super::pgorm_range_types::Slot;
use super::pgorm_range_types::Textrange;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[pgorm(table_name = "measurement")]
pub struct Model {
    #[pgorm(primary_key)]
    pub id: i32,
    pub span: Floatrange,
    pub slot: Option<Slot>,
    pub label: Textrange,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
