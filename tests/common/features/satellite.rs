use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[pgorm(table_name = "satellite")]
pub struct Model {
    #[pgorm(primary_key)]
    pub id: i32,
    pub satellite_name: String,
    #[pgorm(default_value = "2022-01-26 16:24:00")]
    pub launch_date: DateTimeUtc,
    #[pgorm(default_value = "2022-01-26 16:24:00")]
    pub deployment_date: DateTimeLocal,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
