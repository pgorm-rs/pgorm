//! The prelude as a contract.
//!
//! This file imports `pgorm::entity::prelude::*` and, apart from the entity
//! module it queries, nothing else from pgorm. Everything below therefore
//! compiles only while the prelude still carries the names it uses — dropping
//! one is a break for callers who never named it, so it is a break here.

use std::num::NonZeroU64;

use pgorm::entity::prelude::*;
use pgorm::tests_cfg::cake;

#[derive(Debug, PartialEq, Eq, FromQueryResult)]
struct NameOnly {
    name: String,
}

/// The connection vocabulary, named in signatures rather than called: a pool, a
/// checked-out connection, a transaction, and the two traits over them.
async fn _connection_vocabulary(
    pool: &DatabasePool,
    conn: &mut DatabaseConnection,
) -> Result<(), Error> {
    let _: DatabaseConnection = pool.get().await?;
    let txn: DatabaseTransaction<'_> = TransactionTrait::begin(conn).await?;
    let _: u64 = txn.execute("SELECT 1", &[]).await?;
    txn.rollback().await
}

/// The paginator and the loader are traits, so their methods are unreachable
/// unless the prelude carries them.
async fn _paged<C: ConnectionTrait>(db: &C) -> Result<u64, Error> {
    cake::Entity::find()
        .paginate(db, NonZeroU64::new(10).expect("10 is not zero"))
        .num_pages()
        .await
}

// [spec:pgorm:def:entity.prelude+2/test]    a query built, an active model set, a
// statement decoded and a column enumerated, with the prelude as the only import
#[test]
fn prelude_carries_what_a_query_needs() {
    // `QuerySelect`, `QueryFilter`, `QueryOrder` and `QueryTrait`: every method
    // in this chain belongs to a trait, and `as_query` to a fourth.
    let sql = cake::Entity::find()
        .select_only()
        .column(cake::Column::Name)
        .filter(cake::Column::Id.gt(1))
        .filter(Condition::any().add(cake::Column::Name.like("Ch%")))
        .order_by_asc(cake::Column::Id)
        .limit(5)
        .as_query()
        .to_string();
    assert_eq!(
        sql,
        r#"SELECT "cake"."name" FROM "cake" WHERE "cake"."id" > 1 AND "cake"."name" LIKE 'Ch%' ORDER BY "cake"."id" ASC LIMIT 5"#
    );

    // `JoinType` names the join a relation is walked through.
    let joined = cake::Entity::find()
        .join(JoinType::InnerJoin, cake::Relation::Fruit.def())
        .as_query()
        .to_string();
    assert!(joined.contains("INNER JOIN"), "{joined}");

    // The `ActiveValue` vocabulary: the three variants, and the `Value` a set
    // one carries.
    let active = cake::ActiveModel {
        id: NotSet,
        name: Set("Cheesecake".to_owned()),
    };
    assert!(active.id.is_not_set());
    assert_eq!(
        active.name.clone().into_value(),
        Some(Value::from("Cheesecake"))
    );
    let unchanged: ActiveValue<i32> = Unchanged(1);
    assert!(unchanged.is_unchanged());

    // A model converts back to an active model, and an active model tries back
    // to a model.
    let model = cake::Model {
        id: 1,
        name: "Cheesecake".to_owned(),
    };
    let round: cake::ActiveModel = model.clone().into_active_model();
    assert_eq!(round.try_into_model().expect("every field is set"), model);

    // `DecodeSelect` names the row type once, on the statement.
    let _decoded = cake::Entity::find()
        .select_only()
        .column(cake::Column::Name)
        .into_query()
        .into_model::<NameOnly>();

    // `Iterable` is what makes a column enum enumerable.
    assert_eq!(
        cake::Column::iter()
            .map(|c| c.as_str().to_owned())
            .collect::<Vec<_>>(),
        ["id".to_owned(), "name".to_owned()]
    );
}
