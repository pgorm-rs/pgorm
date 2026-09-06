use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, DeriveActiveModel)]
pub struct Model {
    #[pgorm(nulable)]
    pub id: i32,
}

fn main() {}
