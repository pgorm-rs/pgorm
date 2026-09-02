use pgorm::entity::prelude::*;

// `DeriveEntityModel` input MUST be a struct named exactly `Model`.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[pgorm(table_name = "cake")]
pub struct Cake {
    #[pgorm(primary_key)]
    pub id: i32,
}

fn main() {}
