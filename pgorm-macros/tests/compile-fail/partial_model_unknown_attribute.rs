use pgorm::entity::prelude::*;

#[derive(DerivePartialModel)]
#[pgorm(entity = "Entity")]
struct Partial {
    #[pgorm(from_colum = "name")]
    label: String,
}

fn main() {}
