use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[pgorm(table_name = "pi")]
pub struct Model {
    #[pgorm(primary_key)]
    pub id: i32,
    #[pgorm(column_type = "Decimal(Some((11, 10)))")]
    pub decimal: Decimal,
    #[pgorm(column_type = "Decimal(Some((11, 10)))")]
    pub big_decimal: BigDecimal,
    #[pgorm(column_type = "Decimal(Some((11, 10)))")]
    pub decimal_opt: Option<Decimal>,
    #[pgorm(column_type = "Decimal(Some((11, 10)))")]
    pub big_decimal_opt: Option<BigDecimal>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
