//! What the generated `schema_name` attribute is *for*: a live database
//! holding two `item` tables, one in `tenant_a` and one in `public` — which is
//! on the default `search_path` — and the entity pgorm-codegen writes for
//! `CREATE TABLE tenant_a.item` reaching its own.
//!
//! The two halves of the claim: `pgorm-codegen`'s
//! `schema_qualifier_tests::compact_entity_carries_the_source_schema` pins that
//! codegen emits `#[pgorm(schema_name = "tenant_a", table_name = "item")]` for
//! a qualified source table, and this pins that an entity written that way
//! reads and writes `tenant_a.item` while an unqualified one — what codegen
//! used to emit for the same DDL — silently gets `public.item` instead.
#![allow(unused_imports, dead_code)]

pub mod common;

pub use common::{TestContext, setup::*};
use pgorm::entity::prelude::*;
use pretty_assertions::assert_eq;

const NO_PARAMS: [&(dyn tokio_postgres::types::ToSql + Sync); 0] = [];

/// The entity pgorm-codegen generates from `CREATE TABLE tenant_a.item`.
mod qualified {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(schema_name = "tenant_a", table_name = "item")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub id: i32,
        pub label: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// The same table read as if its schema had been discarded — what a generator
/// that drops the qualifier produces, and what `search_path` then resolves.
mod unqualified {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "item")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub id: i32,
        pub label: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

#[pgorm_macros::test]
async fn main() -> Result<(), Error> {
    let ctx = TestContext::new("schema_qualified_tests").await;
    let db = ctx.db.get().await?;

    for statement in [
        "CREATE SCHEMA tenant_a",
        "CREATE TABLE tenant_a.item (id int PRIMARY KEY, label text NOT NULL)",
        "CREATE TABLE public.item (id int PRIMARY KEY, label text NOT NULL)",
        "INSERT INTO tenant_a.item (id, label) VALUES (1, 'tenant_a')",
        "INSERT INTO public.item (id, label) VALUES (1, 'public')",
    ] {
        db.execute_raw(statement, NO_PARAMS).await?;
    }

    let result = generated_entity_targets_its_own_schema(&db).await;

    drop(db);
    ctx.delete().await;
    result
}

// [spec:pgorm:sem:codegen.entity.transform+7/test]    preserving the source
// table's schema qualifier is what keeps generated CRUD off a same-named table
// on the `search_path`
// [spec:pgorm:def:codegen.entity.compact+1/test]
async fn generated_entity_targets_its_own_schema(db: &DatabaseConnection) -> Result<(), Error> {
    // `public` is on the default `search_path` and `tenant_a` is not, so an
    // unqualified name resolves to the other table entirely.
    assert_eq!(
        unqualified::Entity::find().one(db).await?,
        unqualified::Model {
            id: 1,
            label: "public".to_owned(),
        }
    );

    assert_eq!(
        qualified::Entity::find().one(db).await?,
        qualified::Model {
            id: 1,
            label: "tenant_a".to_owned(),
        }
    );

    assert_eq!(
        qualified::Entity::find_by_id(1).one(db).await?,
        qualified::Model {
            id: 1,
            label: "tenant_a".to_owned(),
        }
    );

    // Writes land in the same place reads come from.
    qualified::ActiveModel {
        id: set(2),
        label: set("also tenant_a"),
    }
    .insert(db)
    .await?;

    assert_eq!(qualified::Entity::find().all(db).await?.len(), 2);
    assert_eq!(unqualified::Entity::find().all(db).await?.len(), 1);

    Ok(())
}
