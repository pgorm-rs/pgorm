//! Verification for `DeriveRelation`: which variant attributes are required,
//! which are optional, and what each one contributes to the generated
//! `RelationDef`.

#![allow(dead_code)]

use pgorm::entity::prelude::*;
use pgorm::pgorm_query::{Alias, ConditionType, SharedIden};
use pgorm::{Iden, Identity, RelationType};

mod cake {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[pgorm(table_name = "cake")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        pub name: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        // `has_many` needs neither `from` nor `to`: it reverses the target's
        // `Related<Self>` definition.
        #[pgorm(has_many = "super::fruit::Entity")]
        Fruits,
        // ...but it still accepts them, as does `has_one`.
        #[pgorm(
            has_one = "super::fruit::Entity",
            from = "Column::Id",
            to = "super::fruit::Column::CakeId"
        )]
        TopFruit,
    }

    impl Related<super::fruit::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Fruits.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

mod fruit {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[pgorm(table_name = "fruit")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        pub cake_id: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        // The minimal `belongs_to`: the target path plus the mandatory
        // `from` / `to` pair.
        #[pgorm(
            belongs_to = "super::cake::Entity",
            from = "Column::CakeId",
            to = "super::cake::Column::Id"
        )]
        Cake,
        // The same, plus every optional builder key.
        #[pgorm(
            belongs_to = "super::cake::Entity",
            from = "Column::CakeId",
            to = "super::cake::Column::Id",
            on_update = "Cascade",
            on_delete = "SetNull",
            on_condition = r#"pgorm::pgorm_query::Expr::val(1).eq(1)"#,
            fk_name = "fk_fruit_cake",
            condition_type = "AnY"
        )]
        DecoratedCake,
    }

    impl Related<super::cake::Entity> for Entity {
        fn to() -> RelationDef {
            Relation::Cake.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}

/// A standalone relation enum whose entity is named by the container attribute.
/// `Entity` is deliberately not in scope here, so this only compiles if
/// `#[pgorm(entity = ...)]` is honoured.
mod entity_override {
    use pgorm::entity::prelude::*;

    pub use super::cake::Entity as Renamed;

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    #[pgorm(entity = Renamed)]
    pub enum AltRelation {
        #[pgorm(
            belongs_to = "super::fruit::Entity",
            from = "super::cake::Column::Id",
            to = "super::fruit::Column::CakeId"
        )]
        Fruit,
    }
}

fn cols(id: &Identity) -> Vec<String> {
    match id {
        Identity::Unary(a) => vec![Iden::to_string(&**a)],
        Identity::Binary(a, b) => vec![Iden::to_string(&**a), Iden::to_string(&**b)],
        Identity::Ternary(a, b, c) => vec![
            Iden::to_string(&**a),
            Iden::to_string(&**b),
            Iden::to_string(&**c),
        ],
        Identity::Many(v) => v.iter().map(|i| Iden::to_string(&**i)).collect(),
    }
}

// [spec:pgorm:syn:macros.derive.relation+1/test]    belongs_to + the mandatory from/to
#[test]
fn belongs_to_builds_a_non_owning_relation_def() {
    let def = fruit::Relation::Cake.def();

    // `Entity::belongs_to(target)` => HasOne, not the owner.
    assert_eq!(def.rel_type, RelationType::HasOne);
    assert!(!def.is_owner);
    assert_eq!(def.from_tbl, fruit::Entity.table_ref().into());
    assert_eq!(def.to_tbl, cake::Entity.table_ref().into());
    assert_eq!(
        cols(&def.columns.from_identity()),
        vec!["cake_id".to_owned()]
    );
    assert_eq!(cols(&def.columns.to_identity()), vec!["id".to_owned()]);

    // Everything optional is left alone.
    assert!(def.on_update.is_none());
    assert!(def.on_delete.is_none());
    assert!(def.on_condition.is_none());
    assert_eq!(def.fk_name, None);
    assert_eq!(def.condition_type, ConditionType::All);
}

// [spec:pgorm:syn:macros.derive.relation+1/test]    the optional builder keys
#[test]
fn optional_keys_chain_onto_the_relation_builder() {
    let def = fruit::Relation::DecoratedCake.def();

    assert!(matches!(def.on_update, Some(ForeignKeyAction::Cascade)));
    assert!(matches!(def.on_delete, Some(ForeignKeyAction::SetNull)));
    assert_eq!(def.fk_name.as_deref(), Some("fk_fruit_cake"));
    // `condition_type` is matched case-insensitively.
    assert_eq!(def.condition_type, ConditionType::Any);

    // `on_condition` is wrapped in an `IntoCondition` closure taking the two
    // join-side idens.
    let on_condition = def.on_condition.expect("on_condition should be set");
    let condition = on_condition(
        SharedIden::new(Alias::new("l")),
        SharedIden::new(Alias::new("r")),
    );
    assert_eq!(
        condition,
        pgorm::pgorm_query::IntoCondition::into_condition(pgorm::pgorm_query::Expr::val(1).eq(1))
    );
}

// [spec:pgorm:syn:macros.derive.relation+1/test]    has_many / has_one, where from and to are optional
#[test]
fn has_many_and_has_one_reverse_the_target() {
    let many = cake::Relation::Fruits.def();
    assert_eq!(many.rel_type, RelationType::HasMany);
    assert!(many.is_owner);
    // Reversed: the def reads from cake to fruit.
    assert_eq!(many.from_tbl, cake::Entity.table_ref().into());
    assert_eq!(many.to_tbl, fruit::Entity.table_ref().into());
    assert_eq!(cols(&many.columns.from_identity()), vec!["id".to_owned()]);
    assert_eq!(
        cols(&many.columns.to_identity()),
        vec!["cake_id".to_owned()]
    );

    let one = cake::Relation::TopFruit.def();
    assert_eq!(one.rel_type, RelationType::HasOne);
    assert!(one.is_owner);
    // Explicit `from` / `to` are accepted here too and override the reversal.
    assert_eq!(cols(&one.columns.from_identity()), vec!["id".to_owned()]);
    assert_eq!(cols(&one.columns.to_identity()), vec!["cake_id".to_owned()]);
}

// [spec:pgorm:syn:macros.derive.relation+1/test]    container-level entity override
#[test]
fn the_entity_identifier_is_overridable() {
    let def = entity_override::AltRelation::Fruit.def();
    // The relation was built from `Renamed` (= cake::Entity), not from an
    // `Entity` in scope — there is none.
    assert_eq!(def.from_tbl, cake::Entity.table_ref().into());
    assert_eq!(def.to_tbl, fruit::Entity.table_ref().into());
    assert_eq!(def.rel_type, RelationType::HasOne);
}
