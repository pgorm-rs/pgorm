use pgorm::entity::prelude::*;

#[derive(Debug, Clone, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
#[pgorm(
    rs_type = "String",
    db_type = "Enum",
    enum_name = "stage",
    schema_name = "sales"
)]
pub enum Stage {
    #[pgorm(string_value = "lead")]
    Lead,
    #[pgorm(string_value = "won")]
    Won,
    #[pgorm(string_value = "lost")]
    Lost,
}
