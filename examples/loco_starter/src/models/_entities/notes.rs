use pgorm::entity::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, DeriveEntityModel, Eq, Serialize, Deserialize)]
#[pgorm(table_name = "notes")]
pub struct Model {
    pub created_at: DateTime,
    pub updated_at: DateTime,
    #[pgorm(primary_key)]
    pub id: i32,
    pub title: Option<String>,
    pub content: Option<String>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}
