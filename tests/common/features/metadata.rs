use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[pgorm(table_name = "metadata")]
pub struct Model {
    #[pgorm(primary_key, auto_increment = false)]
    pub uuid: Uuid,
    #[pgorm(column_name = "type", enum_name = "Type")]
    pub ty: String,
    pub key: String,
    pub value: String,
    #[pgorm(column_type = "var_binary(32)")]
    pub bytes: Vec<u8>,
    pub date: Option<Date>,
    pub time: Option<Time>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
