use pgorm::entity::prelude::*;

#[derive(Debug, EnumIter, DeriveActiveEnum)]
#[pgorm(db_type = "Integer")]
pub enum Missing {
    #[pgorm(num_value = 1)]
    One,
}

fn main() {}
