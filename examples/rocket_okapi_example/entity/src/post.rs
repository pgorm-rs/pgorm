use rocket::serde::{Deserialize, Serialize};
use rocket_okapi::okapi::schemars::{self, JsonSchema};
use pgorm::entity::prelude::*;

#[derive(
    Clone, Debug, PartialEq, Eq, DeriveEntityModel, Deserialize, Serialize, FromForm, JsonSchema,
)]
#[serde(crate = "rocket::serde")]
#[pgorm(table_name = "posts")]
pub struct Model {
    #[pgorm(primary_key)]
    pub id: i32,
    pub title: String,
    #[pgorm(column_type = "Text")]
    pub text: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
