//! SeaORM's active enums.

use pgorm::entity::prelude::*;

/// Tea active enum
#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[pgorm(rs_type = "String", db_type = "Enum", enum_name = "tea")]
pub enum Tea {
    /// EverydayTea variant
    #[pgorm(string_value = "EverydayTea")]
    EverydayTea,
    /// BreakfastTea variant
    #[pgorm(string_value = "BreakfastTea")]
    BreakfastTea,
}
