use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[pgorm(table_name = "model")]
pub struct Model {
    #[pgorm(primary_key, auto_increment = false)]
    pub id_1: i32,
    #[pgorm(primary_key, auto_increment = false)]
    pub id_2: String,
    #[pgorm(primary_key, auto_increment = false)]
    pub id_3: f64,
    #[pgorm(primary_key, auto_increment = false)]
    pub id_4: Uuid,
    #[pgorm(primary_key, auto_increment = false)]
    pub id_5: DateTime,
    #[pgorm(primary_key, auto_increment = false)]
    pub id_6: DateTimeWithTimeZone,
    pub owner: String,
    pub name: String,
    pub description: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
