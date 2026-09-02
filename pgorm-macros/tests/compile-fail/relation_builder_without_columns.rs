use pgorm::entity::prelude::*;
use pgorm::tests_cfg::{cake, fruit};

fn main() {
    let _: RelationDef = fruit::Entity::belongs_to(cake::Entity).into();
}
