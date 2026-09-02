use pgorm::entity::prelude::*;

// A `Model` with no `#[pgorm(primary_key)]` field makes `DeriveEntityModel`
// generate exactly this: an empty `PrimaryKey` enum. `DerivePrimaryKey` refuses
// it, which is how the "entity must have a primary key" contract is enforced.
#[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
pub enum PrimaryKey {}

fn main() {}
