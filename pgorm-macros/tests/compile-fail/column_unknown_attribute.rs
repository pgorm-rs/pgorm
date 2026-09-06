use pgorm::entity::prelude::*;

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    #[pgorm(colum_name = "id")]
    Id,
}

fn main() {}
