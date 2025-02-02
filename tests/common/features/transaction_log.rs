use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[pgorm(table_name = "transaction_log")]
pub struct Model {
    #[pgorm(primary_key)]
    pub id: i32,
    pub date: TimeDate,
    pub time: TimeTime,
    pub date_time: TimeDateTime,
    pub date_time_tz: TimeDateTimeWithTimeZone,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
