use pgorm::entity::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[pgorm(rs_type = "String", db_type = "String(StringLen::N(1))")]
pub enum Category {
    #[pgorm(string_value = "B")]
    Big,
    #[pgorm(string_value = "S")]
    Small,
}

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[pgorm(rs_type = "i32", db_type = "Integer")]
pub enum Color {
    #[pgorm(num_value = 0)]
    Black,
    #[pgorm(num_value = 1)]
    White,
}

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[pgorm(rs_type = "String", db_type = "Enum", enum_name = "tea")]
pub enum Tea {
    #[pgorm(string_value = "EverydayTea")]
    EverydayTea,
    #[pgorm(string_value = "BreakfastTea")]
    BreakfastTea,
}
