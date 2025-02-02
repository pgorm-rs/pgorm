use pgorm::entity::prelude::*;
use serde::{Serialize, Deserialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Serialize, Deserialize)]
#[pgorm(table_name = "materials")]
pub struct Model {
    #[pgorm(primary_key)]
    pub id: i32,
    pub created_at: DateTimeWithTimeZone,
    pub updated_at: DateTimeWithTimeZone,
    pub name: String,
    #[pgorm(column_type = "Text", nullable)]
    pub description: Option<String> ,
    pub tag_ids: Vec<u8> ,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
