use pgorm::entity::prelude::*;

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[pgorm(belongs_to = "super::Nothing")]
    Nothing,
}

fn main() {}
