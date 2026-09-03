#![allow(unused_imports, dead_code)]

//! The SQL-building contract of `src/query/`.
//!
//! Every assertion in this file renders a statement through `QueryTrait`, so
//! none of it needs a database: the claims under `query.build` are about what
//! the builders *produce*, and the produced text is the observable.
//!
//! Run the test locally:
//! cargo test --test query_build_tests

use pgorm::pgorm_query::{
    Alias, Asterisk, ConditionType, DeleteStatement, Expr, Func, InsertStatement, IntoCondition,
    IntoIden, LockBehavior, LockType, NullOrdering, OnConflict, QueryBuilder, SelectStatement,
    SimpleExpr, UpdateStatement, Value, Values,
};
use pgorm::tests_cfg::{
    cake, cake_filling, cake_filling_price, entity_linked, filling, fruit, lunch_set,
    sea_orm_active_enums::Tea, vendor,
};
use pgorm::{
    ActiveValue, ColumnTrait, Condition, DbErr, DebugQuery, Delete, DeleteMany, DeleteOne,
    EntityTrait, IdenStatic, Insert, IntoActiveModel, Iterable, JoinType, ModelTrait, Order,
    QueryFilter, QueryOrder, QuerySelect, QueryTrait, Related, RelationTrait, Select,
    SelectColumns, SelectTwo, SelectTwoMany, TryInsert, Update, UpdateMany, UpdateOne,
};
use pretty_assertions::assert_eq;

fn apple() -> cake::ActiveModel {
    cake::ActiveModel {
        id: ActiveValue::Set(1),
        name: ActiveValue::Set("Apple Pie".to_owned()),
    }
}

/// Exercises the three blanket traits over whichever SELECT builder is passed
/// in, proving the shared modification surface really is shared.
fn narrow<Q>(query: Q) -> Q
where
    Q: QuerySelect<QueryStatement = SelectStatement>
        + QueryOrder<QueryStatement = SelectStatement>
        + QueryFilter<QueryStatement = SelectStatement>,
{
    query
        .filter(cake::Column::Id.gt(0))
        .order_by_asc(cake::Column::Id)
        .limit(1)
}

// [spec:pgorm:req:query.build/test]    every builder in the inventory, each
// wrapping exactly one pgorm-query statement reachable through `QueryTrait`,
// and the blanket `QuerySelect`/`QueryOrder`/`QueryFilter` surface
#[test]
fn every_builder_wraps_one_statement() {
    let select: Select<cake::Entity> = cake::Entity::find();
    assert_eq!(
        select.as_query().to_string(QueryBuilder),
        r#"SELECT "cake"."id", "cake"."name" FROM "cake""#
    );
    let _: SelectStatement = select.into_query();

    let select_two: SelectTwo<cake::Entity, fruit::Entity> =
        cake::Entity::find().find_also_related(fruit::Entity);
    let _: SelectStatement = select_two.into_query();

    let select_two_many: SelectTwoMany<cake::Entity, fruit::Entity> =
        cake::Entity::find().find_with_related(fruit::Entity);
    let _: SelectStatement = select_two_many.into_query();

    let insert: Insert<cake::ActiveModel> = Insert::one(apple());
    assert_eq!(
        insert.as_query().to_string(QueryBuilder),
        r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie')"#
    );
    let _: InsertStatement = insert.into_query();

    let try_insert: TryInsert<cake::ActiveModel> = Insert::one(apple()).do_nothing();
    let _: InsertStatement = try_insert.into_query();

    let update_one: UpdateOne<cake::ActiveModel> =
        Update::one(apple()).expect("the primary key is set");
    assert_eq!(
        update_one.as_query().to_string(QueryBuilder),
        r#"UPDATE "cake" SET "name" = 'Apple Pie' WHERE "cake"."id" = 1"#
    );
    let _: UpdateStatement = update_one.into_query();

    let update_many: UpdateMany<cake::Entity> =
        Update::many(cake::Entity).col_expr(cake::Column::Name, Expr::value("Pie"));
    assert_eq!(
        update_many.as_query().to_string(QueryBuilder),
        r#"UPDATE "cake" SET "name" = 'Pie'"#
    );
    let _: UpdateStatement = update_many.into_query();

    let delete_one: DeleteOne<cake::ActiveModel> =
        Delete::one(apple()).expect("the primary key is set");
    assert_eq!(
        delete_one.as_query().to_string(QueryBuilder),
        r#"DELETE FROM "cake" WHERE "cake"."id" = 1"#
    );
    let _: DeleteStatement = delete_one.into_query();

    let delete_many: DeleteMany<cake::Entity> = Delete::many(cake::Entity);
    assert_eq!(
        delete_many.as_query().to_string(QueryBuilder),
        r#"DELETE FROM "cake""#
    );
    let _: DeleteStatement = delete_many.into_query();

    // The same three blanket traits drive all three SELECT builders.
    assert_eq!(
        narrow(cake::Entity::find())
            .as_query()
            .to_string(QueryBuilder),
        r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" > 0 ORDER BY "cake"."id" ASC LIMIT 1"#
    );
    assert!(
        narrow(cake::Entity::find().find_also_related(fruit::Entity))
            .as_query()
            .to_string(QueryBuilder)
            .ends_with(r#"WHERE "cake"."id" > 0 ORDER BY "cake"."id" ASC LIMIT 1"#)
    );
    assert!(
        narrow(cake::Entity::find().find_with_related(fruit::Entity))
            .as_query()
            .to_string(QueryBuilder)
            .ends_with(
                r#"WHERE "cake"."id" > 0 ORDER BY "cake"."id" ASC, "cake"."id" ASC LIMIT 1"#
            )
    );
}

// [spec:pgorm:sem:query.build.select-defaults/test]    the select list is every
// column in `Column::iter()` order through `select_as`, the FROM is the
// entity's table ref, and nothing else is applied
#[test]
fn select_new_has_only_columns_and_from() {
    assert_eq!(
        cake::Column::iter()
            .map(|c| c.as_str().to_owned())
            .collect::<Vec<_>>(),
        ["id", "name"]
    );
    assert_eq!(
        cake::Entity::find().as_query().to_string(QueryBuilder),
        r#"SELECT "cake"."id", "cake"."name" FROM "cake""#
    );

    // `select_as` casts an enum column to text; the rest pass through.
    assert_eq!(
        lunch_set::Entity::find().as_query().to_string(QueryBuilder),
        [
            r#"SELECT "lunch_set"."id", "lunch_set"."name","#,
            r#"CAST("lunch_set"."tea" AS text) FROM "lunch_set""#,
        ]
        .join(" ")
    );

    // The FROM clause is `E::default().table_ref()`, schema included.
    assert_eq!(
        cake_filling_price::Entity::find()
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake_filling_price"."cake_id", "cake_filling_price"."filling_id","#,
            r#""cake_filling_price"."price" FROM "public"."cake_filling_price""#,
        ]
        .join(" ")
    );

    // No default WHERE / ORDER BY / GROUP BY / LIMIT / OFFSET.
    let sql = cake::Entity::find().as_query().to_string(QueryBuilder);
    for clause in ["WHERE", "ORDER BY", "GROUP BY", "HAVING", "LIMIT", "OFFSET"] {
        assert!(!sql.contains(clause), "unexpected {clause} in {sql}");
    }
}

// [spec:pgorm:def:query.build.query-trait/test]    `query`, `as_query`,
// `into_query` and `build` — the last rendering PostgreSQL text with `$n`
// placeholders and a parallel `Values`, taking no backend argument
#[test]
fn query_trait_exposes_the_statement_four_ways() {
    let mut select = cake::Entity::find().filter(cake::Column::Id.eq(3));

    let (sql, values) = select.build();
    assert_eq!(
        sql,
        r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" = $1"#
    );
    assert_eq!(values, Values(vec![Value::Int(Some(3))]));

    // `as_query` is shared access: the same statement, values inlined.
    assert_eq!(
        select.as_query().to_string(QueryBuilder),
        r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" = 3"#
    );

    // `query` is mutable access to the very same statement.
    QueryTrait::query(&mut select).and_where(cake::Column::Name.like("%pie%"));
    assert_eq!(
        select.build().0,
        r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" = $1 AND "cake"."name" LIKE $2"#
    );

    // `into_query` hands over ownership of that statement.
    let statement: SelectStatement = select.into_query();
    assert_eq!(
        statement.to_string(QueryBuilder),
        r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" = 3 AND "cake"."name" LIKE '%pie%'"#
    );
}

// [spec:pgorm:def:query.build.query-trait/test]    `apply_if` runs the closure
// only for `Some`, leaving the fluent chain unbroken for `None`
#[test]
fn apply_if_runs_only_on_some() {
    assert_eq!(
        cake::Entity::find()
            .apply_if(Some(3), |query, v| query.filter(cake::Column::Id.eq(v)))
            .apply_if(Some(100), QuerySelect::limit)
            .apply_if(None, QuerySelect::offset::<Option<u64>>)
            .apply_if(None::<i32>, |query, v| query.filter(cake::Column::Id.eq(v)))
            .as_query()
            .to_string(QueryBuilder),
        r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" = 3 LIMIT 100"#
    );
}

// [spec:pgorm:def:query.build.query-trait/test]    `IntoSimpleExpr` accepts a
// `ColumnTrait` (yielding a table-qualified column reference), an `Expr` and a
// `SimpleExpr` (identity)
#[test]
fn into_simple_expr_accepts_three_shapes() {
    let by_column = cake::Entity::find().order_by_asc(cake::Column::Name);
    let by_expr = cake::Entity::find().order_by_asc(Expr::col((cake::Entity, cake::Column::Name)));
    let by_simple: SimpleExpr = Expr::col((cake::Entity, cake::Column::Name)).into();

    let expected =
        r#"SELECT "cake"."id", "cake"."name" FROM "cake" ORDER BY "cake"."name" ASC"#.to_owned();
    assert_eq!(by_column.as_query().to_string(QueryBuilder), expected);
    assert_eq!(by_expr.as_query().to_string(QueryBuilder), expected);
    assert_eq!(
        cake::Entity::find()
            .order_by_asc(by_simple)
            .as_query()
            .to_string(QueryBuilder),
        expected
    );

    // A non-column `SimpleExpr` passes through unchanged too.
    assert_eq!(
        cake::Entity::find()
            .group_by(cake::Column::Id.count())
            .as_query()
            .to_string(QueryBuilder),
        r#"SELECT "cake"."id", "cake"."name" FROM "cake" GROUP BY COUNT("cake"."id")"#
    );
}

// [spec:pgorm:sem:query.build.filter/test]    repeated `filter` calls AND
// together; condition trees, `add_option` and raw pgorm-query expressions all
// arrive through the same entry point
#[test]
fn filter_accumulates_and_accepts_trees() {
    assert_eq!(
        cake::Entity::find()
            .filter(cake::Column::Id.eq(4))
            .filter(cake::Column::Id.eq(5))
            .as_query()
            .to_string(QueryBuilder),
        r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" = 4 AND "cake"."id" = 5"#
    );

    assert_eq!(
        cake::Entity::find()
            .filter(
                Condition::any()
                    .add(cake::Column::Id.eq(4))
                    .add(cake::Column::Id.eq(5))
            )
            .filter(Condition::all().add(cake::Column::Name.contains("cheese")))
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id", "cake"."name" FROM "cake""#,
            r#"WHERE ("cake"."id" = 4 OR "cake"."id" = 5) AND "cake"."name" LIKE '%cheese%'"#,
        ]
        .join(" ")
    );

    // `add_option` drops a `None` predicate without breaking the chain.
    let absent: Option<String> = None;
    assert_eq!(
        cake::Entity::find()
            .filter(Condition::all().add_option(absent.map(|n| cake::Column::Name.contains(&n))))
            .filter(cake::Column::Id.is_in([4, 5]))
            .as_query()
            .to_string(QueryBuilder),
        r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" IN (4, 5)"#
    );

    // A raw pgorm-query expression is accepted verbatim.
    assert_eq!(
        fruit::Entity::find()
            .filter(Expr::col(fruit::Column::CakeId).is_null())
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "fruit"."id", "fruit"."name", "fruit"."cake_id" FROM "fruit""#,
            r#"WHERE "cake_id" IS NULL"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:sem:query.build.filter/test]    `belongs_to` emits one equality
// per primary-key column of the model's entity; `belongs_to_tbl_alias`
// qualifies the same columns with a table alias
#[test]
fn belongs_to_filters_every_primary_key_column() {
    let single = cake::Model {
        id: 12,
        name: String::new(),
    };
    assert_eq!(
        fruit::Entity::find()
            .belongs_to(&single)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "fruit"."id", "fruit"."name", "fruit"."cake_id" FROM "fruit""#,
            r#"WHERE "cake"."id" = 12"#,
        ]
        .join(" ")
    );

    // A composite primary key contributes one equality per column, in
    // `PrimaryKey::iter()` order.
    let composite = cake_filling::Model {
        cake_id: 2,
        filling_id: 3,
    };
    assert_eq!(
        filling::Entity::find()
            .belongs_to(&composite)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "filling"."id", "filling"."name", "filling"."vendor_id" FROM "filling""#,
            r#"WHERE "cake_filling"."cake_id" = 2 AND "cake_filling"."filling_id" = 3"#,
        ]
        .join(" ")
    );

    assert_eq!(
        filling::Entity::find()
            .belongs_to_tbl_alias(&composite, "r0")
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "filling"."id", "filling"."name", "filling"."vendor_id" FROM "filling""#,
            r#"WHERE "r0"."cake_id" = 2 AND "r0"."filling_id" = 3"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:sem:query.build.modifiers+2/test]    `select_only` clears the list;
// `column`/`columns` re-add through `select_as`; `column_as`, `expr_as`,
// `tbl_col_as`, `expr` and `exprs` append explicit expressions, and
// `SelectColumns` re-exposes the first two
#[test]
fn select_list_modifiers_rewrite_the_list() {
    assert_eq!(
        cake::Entity::find()
            .select_only()
            .columns([cake::Column::Id, cake::Column::Name])
            .column_as(cake::Column::Id.count(), "count")
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id", "cake"."name", COUNT("cake"."id") AS "count""#,
            r#"FROM "cake""#,
        ]
        .join(" ")
    );

    // `column` applies the same enum cast as the default select list.
    assert_eq!(
        lunch_set::Entity::find()
            .select_only()
            .column(lunch_set::Column::Tea)
            .as_query()
            .to_string(QueryBuilder),
        r#"SELECT CAST("lunch_set"."tea" AS text) FROM "lunch_set""#
    );

    assert_eq!(
        cake::Entity::find()
            .select_only()
            .expr(Expr::col((cake::Entity, cake::Column::Id)))
            .exprs([Expr::col((cake::Entity, cake::Column::Name))])
            .expr_as(
                Func::upper(Expr::col((cake::Entity, cake::Column::Name))),
                "name_upper"
            )
            .tbl_col_as((cake::Entity, cake::Column::Name), "cake_name")
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id", "cake"."name", UPPER("cake"."name") AS "name_upper","#,
            r#""cake"."name" AS "cake_name" FROM "cake""#,
        ]
        .join(" ")
    );

    // `SelectColumns` is the same pair of methods under partial-model names.
    assert_eq!(
        cake::Entity::find()
            .select_only()
            .select_column(cake::Column::Name)
            .select_column_as(cake::Column::Id, "cake_id")
            .as_query()
            .to_string(QueryBuilder),
        r#"SELECT "cake"."name", "cake"."id" AS "cake_id" FROM "cake""#
    );
}

// [spec:pgorm:sem:query.build.modifiers+2/test]    rendering a cleared select
// list still emits the text as written — `to_string` and `build` have no
// `Result` channel, so the empty projection is refused at execution instead
// (see `empty_select_tests.rs`)
#[test]
fn cleared_select_list_renders_verbatim() {
    let query = cake::Entity::find().select_only();

    assert_eq!(
        query.as_query().to_string(QueryBuilder),
        r#"SELECT  FROM "cake""#
    );
    assert_eq!(query.build().0, r#"SELECT  FROM "cake""#);
}

// [spec:pgorm:sem:query.build.modifiers+2/test]    `limit`/`offset` take
// `Into<Option<u64>>`: the last `Some` wins and `None` removes the clause
#[test]
fn limit_and_offset_last_call_wins() {
    assert_eq!(
        cake::Entity::find()
            .limit(10)
            .offset(5)
            .as_query()
            .to_string(QueryBuilder),
        r#"SELECT "cake"."id", "cake"."name" FROM "cake" LIMIT 10 OFFSET 5"#
    );

    assert_eq!(
        cake::Entity::find()
            .limit(Some(10))
            .limit(Some(20))
            .offset(Some(1))
            .offset(Some(2))
            .as_query()
            .to_string(QueryBuilder),
        r#"SELECT "cake"."id", "cake"."name" FROM "cake" LIMIT 20 OFFSET 2"#
    );

    assert_eq!(
        cake::Entity::find()
            .limit(10)
            .offset(5)
            .limit(None)
            .offset(None)
            .as_query()
            .to_string(QueryBuilder),
        r#"SELECT "cake"."id", "cake"."name" FROM "cake""#
    );
}

// [spec:pgorm:sem:query.build.modifiers+2/test]    `group_by` adds GROUP BY,
// `having` accumulates AND-ed conditions, `distinct` / `distinct_on` and the
// four locking helpers each add their clause
#[test]
fn grouping_distinct_and_locking_clauses() {
    assert_eq!(
        cake::Entity::find()
            .select_only()
            .column_as(cake::Column::Id.count(), "count")
            .group_by(cake::Column::Name)
            .having(cake::Column::Id.gt(4))
            .having(cake::Column::Id.lt(9))
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT COUNT("cake"."id") AS "count" FROM "cake""#,
            r#"GROUP BY "cake"."name" HAVING "cake"."id" > 4 AND "cake"."id" < 9"#,
        ]
        .join(" ")
    );

    assert_eq!(
        cake::Entity::find()
            .distinct()
            .as_query()
            .to_string(QueryBuilder),
        r#"SELECT DISTINCT "cake"."id", "cake"."name" FROM "cake""#
    );
    assert_eq!(
        cake::Entity::find()
            .distinct_on([(cake::Entity, cake::Column::Name)])
            .as_query()
            .to_string(QueryBuilder),
        r#"SELECT DISTINCT ON ("cake"."name") "cake"."id", "cake"."name" FROM "cake""#
    );

    let locked = |query: Select<cake::Entity>| query.as_query().to_string(QueryBuilder);
    assert!(locked(cake::Entity::find().lock_shared()).ends_with("FOR SHARE"));
    assert!(locked(cake::Entity::find().lock_exclusive()).ends_with("FOR UPDATE"));
    assert!(
        locked(cake::Entity::find().lock(LockType::NoKeyUpdate)).ends_with("FOR NO KEY UPDATE")
    );
    assert!(
        locked(cake::Entity::find().lock_with_behavior(LockType::Update, LockBehavior::SkipLocked))
            .ends_with("FOR UPDATE SKIP LOCKED")
    );
}

// [spec:pgorm:sem:query.build.modifiers+2/test]    ORDER BY expressions append in
// call order and are never deduplicated
#[test]
fn order_by_appends_and_never_dedups() {
    assert_eq!(
        cake::Entity::find()
            .order_by(cake::Column::Id, Order::Asc)
            .order_by_desc(cake::Column::Name)
            .order_by_asc(cake::Column::Id)
            .order_by_with_nulls(cake::Column::Name, Order::Asc, NullOrdering::First)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id", "cake"."name" FROM "cake" ORDER BY "cake"."id" ASC,"#,
            r#""cake"."name" DESC, "cake"."id" ASC, "cake"."name" ASC NULLS FIRST"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:sem:query.build.join+2/test]    `join` targets `to_tbl`, `join_rev`
// targets `from_tbl`, and `join_as`/`join_as_rev` re-alias the joined table
// first; the ON condition is one equality per declared column pair
#[test]
fn join_direction_and_alias_choice() {
    assert_eq!(
        cake::Entity::find()
            .join(JoinType::LeftJoin, cake::Relation::Fruit.def())
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id", "cake"."name" FROM "cake""#,
            r#"LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
        ]
        .join(" ")
    );

    assert_eq!(
        fruit::Entity::find()
            .join_rev(JoinType::InnerJoin, cake::Relation::Fruit.def())
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "fruit"."id", "fruit"."name", "fruit"."cake_id" FROM "fruit""#,
            r#"INNER JOIN "cake" ON "cake"."id" = "fruit"."cake_id""#,
        ]
        .join(" ")
    );

    // A composite `Identity` becomes one equality per zipped column pair.
    assert_eq!(
        cake_filling::Entity::find()
            .join(
                JoinType::InnerJoin,
                cake_filling_price::Relation::CakeFilling.def()
            )
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake_filling"."cake_id", "cake_filling"."filling_id" FROM "cake_filling""#,
            r#"INNER JOIN "cake_filling" ON"#,
            r#""cake_filling_price"."cake_id" = "cake_filling"."cake_id" AND"#,
            r#""cake_filling_price"."filling_id" = "cake_filling"."filling_id""#,
        ]
        .join(" ")
    );

    // The alias, when the table ref carries one, replaces the bare identifier
    // on both sides of the ON condition.
    assert_eq!(
        cake::Entity::find()
            .join_as(
                JoinType::LeftJoin,
                cake::Relation::Fruit.def(),
                Alias::new("f")
            )
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id", "cake"."name" FROM "cake""#,
            r#"LEFT JOIN "fruit" AS "f" ON "cake"."id" = "f"."cake_id""#,
        ]
        .join(" ")
    );
    assert_eq!(
        fruit::Entity::find()
            .join_as_rev(
                JoinType::LeftJoin,
                cake::Relation::Fruit.def(),
                Alias::new("c")
            )
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "fruit"."id", "fruit"."name", "fruit"."cake_id" FROM "fruit""#,
            r#"LEFT JOIN "cake" AS "c" ON "c"."id" = "fruit"."cake_id""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:sem:query.build.join+2/test]    `condition_type` picks `all` or
// `any` for the ON condition, and the `on_condition` closure is added to it
#[test]
fn join_condition_type_and_custom_predicate() {
    assert_eq!(
        cake::Entity::find()
            .join(JoinType::LeftJoin, cake::Relation::TropicalFruit.def())
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id", "cake"."name" FROM "cake" LEFT JOIN "fruit" ON"#,
            r#""cake"."id" = "fruit"."cake_id" AND "fruit"."name" LIKE '%tropical%'"#,
        ]
        .join(" ")
    );

    assert_eq!(
        cake::Entity::find()
            .join(JoinType::LeftJoin, cake::Relation::OrTropicalFruit.def())
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id", "cake"."name" FROM "cake" LEFT JOIN "fruit" ON"#,
            r#""cake"."id" = "fruit"."cake_id" OR "fruit"."name" LIKE '%tropical%'"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:sem:query.build.join+2/test]    the `Related` helpers join the
// junction relation first when a `via` exists, and `reverse_join` walks the
// relation backwards
#[test]
fn related_helpers_join_via_then_target() {
    assert_eq!(
        cake::Entity::find()
            .left_join(fruit::Entity)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id", "cake"."name" FROM "cake""#,
            r#"LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
        ]
        .join(" ")
    );

    // cake -> filling has a `via` junction, joined ahead of the target.
    assert_eq!(
        cake::Entity::find()
            .inner_join(filling::Entity)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id", "cake"."name" FROM "cake""#,
            r#"INNER JOIN "cake_filling" ON "cake"."id" = "cake_filling"."cake_id""#,
            r#"INNER JOIN "filling" ON "cake_filling"."filling_id" = "filling"."id""#,
        ]
        .join(" ")
    );

    assert_eq!(
        cake::Entity::find()
            .right_join(fruit::Entity)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id", "cake"."name" FROM "cake""#,
            r#"RIGHT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
        ]
        .join(" ")
    );

    assert_eq!(
        fruit::Entity::find()
            .reverse_join(cake::Entity)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "fruit"."id", "fruit"."name", "fruit"."cake_id" FROM "fruit""#,
            r#"INNER JOIN "cake" ON "cake"."id" = "fruit"."cake_id""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:sem:query.build.combine+1/test]    `select_also` / `select_with`
// rewrite E's select list with the `A_` prefix (alias, plain column and
// `AsEnum`-wrapped column alike) and append every F column as `B_<column>`
#[test]
fn combine_prefixes_both_column_sets() {
    assert_eq!(
        cake::Entity::find()
            .column_as(cake::Column::Id, "B")
            .left_join(fruit::Entity)
            .select_also(fruit::Entity)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name", "cake"."id" AS "A_B","#,
            r#""fruit"."id" AS "B_id", "fruit"."name" AS "B_name","#,
            r#""fruit"."cake_id" AS "B_cake_id""#,
            r#"FROM "cake" LEFT JOIN "fruit" ON "cake"."id" = "fruit"."cake_id""#,
        ]
        .join(" ")
    );

    // An `AsEnum`-wrapped column takes the name of the wrapped column.
    assert_eq!(
        lunch_set::Entity::find()
            .select_also(vendor::Entity)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "lunch_set"."id" AS "A_id", "lunch_set"."name" AS "A_name","#,
            r#"CAST("lunch_set"."tea" AS text) AS "A_tea","#,
            r#""vendor"."id" AS "B_id", "vendor"."name" AS "B_name" FROM "lunch_set""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:sem:query.build.combine+1/test]    `SelectTwoMany::new` appends the
// primary-key ORDER BY that keeps a left model's rows adjacent; `SelectTwo`
// adds no ordering. `find_also_related` / `find_with_related` are exactly
// `left_join` plus `select_also` / `select_with`
#[test]
fn select_with_orders_by_primary_key() {
    let also = cake::Entity::find()
        .find_also_related(fruit::Entity)
        .as_query()
        .to_string(QueryBuilder);
    let with = cake::Entity::find()
        .find_with_related(fruit::Entity)
        .as_query()
        .to_string(QueryBuilder);

    assert!(!also.contains("ORDER BY"), "{also}");
    assert_eq!(with, format!(r#"{also} ORDER BY "cake"."id" ASC"#));

    assert_eq!(
        also,
        cake::Entity::find()
            .left_join(fruit::Entity)
            .select_also(fruit::Entity)
            .as_query()
            .to_string(QueryBuilder)
    );
    assert_eq!(
        with,
        cake::Entity::find()
            .left_join(fruit::Entity)
            .select_with(fruit::Entity)
            .as_query()
            .to_string(QueryBuilder)
    );

    // One ORDER BY term per primary-key column.
    let composite = cake_filling::Entity::find()
        .find_with_related(cake_filling_price::Entity)
        .as_query()
        .to_string(QueryBuilder);
    assert!(
        composite
            .ends_with(r#"ORDER BY "cake_filling"."cake_id" ASC, "cake_filling"."filling_id" ASC"#),
        "{composite}"
    );
}

// [spec:pgorm:sem:query.build.combine+1/test]    a `Linked` chain LEFT JOINs each
// hop as `r{i}` (joining from `r{i-1}`, or the base table at i = 0) and selects
// the final target's columns from the last alias; `find_with_linked` skips the
// primary-key ORDER BY that `find_with_related` adds
#[test]
fn find_linked_aliases_every_hop() {
    assert_eq!(
        cake::Entity::find()
            .find_also_linked(entity_linked::CakeToFillingVendor)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name","#,
            r#""r2"."id" AS "B_id", "r2"."name" AS "B_name""#,
            r#"FROM "cake""#,
            r#"LEFT JOIN "cake_filling" AS "r0" ON "cake"."id" = "r0"."cake_id""#,
            r#"LEFT JOIN "filling" AS "r1" ON "r0"."filling_id" = "r1"."id""#,
            r#"LEFT JOIN "vendor" AS "r2" ON "r1"."vendor_id" = "r2"."id""#,
        ]
        .join(" ")
    );

    // Custom `on_condition` closures ride along on the aliased hop.
    assert_eq!(
        cake::Entity::find()
            .find_also_linked(entity_linked::CheeseCakeToFillingVendor)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name","#,
            r#""r2"."id" AS "B_id", "r2"."name" AS "B_name""#,
            r#"FROM "cake""#,
            r#"LEFT JOIN "cake_filling" AS "r0" ON "cake"."id" = "r0"."cake_id" AND "cake"."name" LIKE '%cheese%'"#,
            r#"LEFT JOIN "filling" AS "r1" ON "r0"."filling_id" = "r1"."id""#,
            r#"LEFT JOIN "vendor" AS "r2" ON "r1"."vendor_id" = "r2"."id""#,
        ]
        .join(" ")
    );

    // Unlike `find_with_related`, the linked `SelectTwoMany` has no ORDER BY.
    let with_linked = cake::Entity::find()
        .find_with_linked(entity_linked::CakeToFillingVendor)
        .as_query()
        .to_string(QueryBuilder);
    assert!(!with_linked.contains("ORDER BY"), "{with_linked}");
    assert_eq!(
        with_linked,
        cake::Entity::find()
            .find_also_linked(entity_linked::CakeToFillingVendor)
            .as_query()
            .to_string(QueryBuilder)
    );
}

// [spec:pgorm:sem:query.build.combine+1/test]    an unaliased asterisk names no
// single column, so it is carried through unprefixed rather than aliased
#[test]
fn combine_leaves_asterisk_unprefixed() {
    assert_eq!(
        cake::Entity::find()
            .expr(Expr::col(Asterisk))
            .select_also(fruit::Entity)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name", *,"#,
            r#""fruit"."id" AS "B_id", "fruit"."name" AS "B_name", "fruit"."cake_id" AS "B_cake_id""#,
            r#"FROM "cake""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:sem:query.build.combine+1/test]    nor does an unaliased expression
// that is neither a column nor an `AsEnum`-wrapped column
#[test]
fn combine_leaves_bare_expression_unprefixed() {
    assert_eq!(
        cake::Entity::find()
            .expr(Func::upper(Expr::col((cake::Entity, cake::Column::Name))))
            .select_also(fruit::Entity)
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"SELECT "cake"."id" AS "A_id", "cake"."name" AS "A_name", UPPER("cake"."name"),"#,
            r#""fruit"."id" AS "B_id", "fruit"."name" AS "B_name", "fruit"."cake_id" AS "B_cake_id""#,
            r#"FROM "cake""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:sem:query.build.insert+1/test]    `Insert::new` renders a valid
// DEFAULT VALUES statement before any model is added, and `one`/`many` /
// `add`/`add_many` take anything `IntoActiveModel`
#[test]
fn insert_new_is_a_default_values_statement() {
    // `or_default_values()` renders as a one-row VALUES list of DEFAULT rather
    // than the `DEFAULT VALUES` spelling; PostgreSQL defaults the columns that
    // the list does not reach, so the statement is still valid.
    assert_eq!(
        cake::Entity::insert_many(std::iter::empty::<cake::ActiveModel>())
            .as_query()
            .to_string(QueryBuilder),
        r#"INSERT INTO "cake" VALUES (DEFAULT)"#
    );

    // A Model is converted to an ActiveModel on the way in.
    let from_model = Insert::<cake::ActiveModel>::one(cake::Model {
        id: 1,
        name: "Apple Pie".to_owned(),
    });
    assert_eq!(
        from_model.as_query().to_string(QueryBuilder),
        r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie')"#
    );

    assert_eq!(
        Insert::<cake::ActiveModel>::many([
            cake::Model {
                id: 1,
                name: "Apple Pie".to_owned(),
            },
            cake::Model {
                id: 2,
                name: "Orange Scone".to_owned(),
            },
        ])
        .as_query()
        .to_string(QueryBuilder),
        r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie'), (2, 'Orange Scone')"#
    );
}

// [spec:pgorm:sem:query.build.insert+1/test]    `add` writes `Set` and
// `Unchanged` columns through `col.save_as` and omits `NotSet` ones entirely
#[test]
fn insert_add_omits_not_set_columns() {
    assert_eq!(
        Insert::one(cake::ActiveModel {
            id: ActiveValue::NotSet,
            name: ActiveValue::Set("Apple Pie".to_owned()),
        })
        .as_query()
        .to_string(QueryBuilder),
        r#"INSERT INTO "cake" ("name") VALUES ('Apple Pie')"#
    );

    assert_eq!(
        Insert::one(cake::ActiveModel {
            id: ActiveValue::Unchanged(7),
            name: ActiveValue::Set("Apple Pie".to_owned()),
        })
        .as_query()
        .to_string(QueryBuilder),
        r#"INSERT INTO "cake" ("id", "name") VALUES (7, 'Apple Pie')"#
    );

    // `save_as` casts an enum value back to its database type.
    assert_eq!(
        Insert::one(lunch_set::ActiveModel {
            id: ActiveValue::NotSet,
            name: ActiveValue::Set("Set A".to_owned()),
            tea: ActiveValue::Set(Tea::EverydayTea),
        })
        .as_query()
        .to_string(QueryBuilder),
        r#"INSERT INTO "lunch_set" ("name", "tea") VALUES ('Set A', CAST('EverydayTea' AS tea))"#
    );
}

// [spec:pgorm:sem:query.build.insert+1/test]    `on_conflict` attaches the given
// pgorm-query clause verbatim
#[test]
fn insert_on_conflict_is_attached_verbatim() {
    assert_eq!(
        cake::Entity::insert(apple())
            .on_conflict(OnConflict::column(cake::Column::Name).update_column(cake::Column::Name))
            .as_query()
            .to_string(QueryBuilder),
        [
            r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie')"#,
            r#"ON CONFLICT ("name") DO UPDATE SET "name" = "excluded"."name""#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:req:query.build.insert.uniform-columns+1/test]    models sharing
// a presence bitmap merge into one multi-row VALUES list
#[test]
fn insert_many_shares_one_column_list() {
    assert_eq!(
        Insert::<cake::ActiveModel>::many([
            cake::ActiveModel {
                id: ActiveValue::NotSet,
                name: ActiveValue::Set("Apple".to_owned()),
            },
            cake::ActiveModel {
                id: ActiveValue::NotSet,
                name: ActiveValue::Set("Orange".to_owned()),
            },
        ])
        .as_query()
        .to_string(QueryBuilder),
        r#"INSERT INTO "cake" ("name") VALUES ('Apple'), ('Orange')"#
    );
}

// [spec:pgorm:req:query.build.insert.uniform-columns+1/test]    a model whose
// presence differs from the first one is recorded as a mismatch, naming the
// column it does not share, and contributes nothing to the statement
#[test]
fn insert_many_rejects_mismatched_columns() {
    let mismatched = || {
        Insert::<cake::ActiveModel>::many([
            cake::ActiveModel {
                id: ActiveValue::NotSet,
                name: ActiveValue::Set("Apple".to_owned()),
            },
            cake::ActiveModel {
                id: ActiveValue::Set(2),
                name: ActiveValue::Set("Orange".to_owned()),
            },
        ])
    };

    let err = mismatched()
        .ensure_uniform_columns()
        .expect_err("the second model sets a column the first does not");
    assert_eq!(
        err.to_string(),
        "Query Error: models added to one insert do not share a column set: \
         `id` is set in a later model but not in the first"
    );

    assert_eq!(
        mismatched().as_query().to_string(QueryBuilder),
        r#"INSERT INTO "cake" ("name") VALUES ('Apple')"#
    );
}

// [spec:pgorm:sem:query.build.insert+1/test]    a model with nothing set
// contributes a default-values row rather than an arity-zero column and value
// list, and one such row per model
#[test]
fn all_not_set_model_renders_a_default_row() {
    let blank = || cake::ActiveModel {
        id: ActiveValue::NotSet,
        name: ActiveValue::NotSet,
    };

    assert_eq!(
        Insert::<cake::ActiveModel>::one(blank())
            .as_query()
            .to_string(QueryBuilder),
        r#"INSERT INTO "cake" VALUES (DEFAULT)"#
    );

    assert_eq!(
        Insert::<cake::ActiveModel>::many([blank(), blank(), blank()])
            .as_query()
            .to_string(QueryBuilder),
        r#"INSERT INTO "cake" VALUES (DEFAULT), (DEFAULT), (DEFAULT)"#
    );
}

// [spec:pgorm:req:query.build.insert.uniform-columns+1/test]    a first model
// that sets nothing is a column set like any other: a later model that sets a
// column mismatches it
#[test]
fn insert_many_rejects_a_blank_first_model() {
    let err = Insert::<cake::ActiveModel>::many([
        cake::ActiveModel {
            id: ActiveValue::NotSet,
            name: ActiveValue::NotSet,
        },
        cake::ActiveModel {
            id: ActiveValue::NotSet,
            name: ActiveValue::Set("Orange".to_owned()),
        },
    ])
    .ensure_uniform_columns()
    .expect_err("the second model sets a column the blank first one does not");

    assert_eq!(
        err.to_string(),
        "Query Error: models added to one insert do not share a column set: \
         `name` is set in a later model but not in the first"
    );
}

// [spec:pgorm:sem:query.build.insert.empty-failsafe+1/test]    `do_nothing` and
// `on_empty_do_nothing` convert to `TryInsert` without touching the statement,
// while `on_conflict_do_nothing` first attaches ON CONFLICT on the primary key
#[test]
fn try_insert_conversions_and_conflict_clause() {
    let plain = Insert::one(apple()).as_query().to_string(QueryBuilder);

    assert_eq!(
        Insert::one(apple())
            .do_nothing()
            .as_query()
            .to_string(QueryBuilder),
        plain
    );
    assert_eq!(
        Insert::one(apple())
            .on_empty_do_nothing()
            .as_query()
            .to_string(QueryBuilder),
        plain
    );

    assert_eq!(
        Insert::one(apple())
            .on_conflict_do_nothing()
            .as_query()
            .to_string(QueryBuilder),
        format!(r#"{plain} ON CONFLICT ("id") DO NOTHING"#)
    );

    // Every primary-key column takes part in the conflict target.
    assert_eq!(
        Insert::one(cake_filling::ActiveModel {
            cake_id: ActiveValue::Set(1),
            filling_id: ActiveValue::Set(2),
        })
        .on_conflict_do_nothing()
        .as_query()
        .to_string(QueryBuilder),
        [
            r#"INSERT INTO "cake_filling" ("cake_id", "filling_id") VALUES (1, 2)"#,
            r#"ON CONFLICT ("cake_id", "filling_id") DO NOTHING"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:sem:query.build.update+2/test]    `Update::one` filters on every
// primary-key column and SETs only `Set`, non-key columns
#[test]
fn update_one_sets_changed_non_key_columns() {
    assert_eq!(
        Update::one(cake::ActiveModel {
            id: ActiveValue::Set(1),
            name: ActiveValue::Set("Apple Pie".to_owned()),
        })
        .expect("the primary key is set")
        .as_query()
        .to_string(QueryBuilder),
        r#"UPDATE "cake" SET "name" = 'Apple Pie' WHERE "cake"."id" = 1"#
    );

    // An `Unchanged` primary key still supplies the filter, and an `Unchanged`
    // attribute is left out of the SET clause.
    assert_eq!(
        Update::one(fruit::ActiveModel {
            id: ActiveValue::Unchanged(1),
            name: ActiveValue::Set("Apple".to_owned()),
            cake_id: ActiveValue::Unchanged(Some(2)),
        })
        .expect("the primary key is unchanged, not unset")
        .as_query()
        .to_string(QueryBuilder),
        r#"UPDATE "fruit" SET "name" = 'Apple' WHERE "fruit"."id" = 1"#
    );

    // Composite keys contribute one equality each, and a `Set` primary key is
    // still never written into the SET clause.
    assert_eq!(
        Update::one(cake_filling_price::ActiveModel {
            cake_id: ActiveValue::Set(1),
            filling_id: ActiveValue::Set(2),
            price: ActiveValue::Set(rust_decimal::Decimal::ONE),
        })
        .expect("both primary-key columns are set")
        .as_query()
        .to_string(QueryBuilder),
        [
            r#"UPDATE "public"."cake_filling_price" SET "price" = 1"#,
            r#"WHERE "cake_filling_price"."cake_id" = 1"#,
            r#"AND "cake_filling_price"."filling_id" = 2"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:sem:query.build.update+2/test]    a `NotSet` primary key has no
// filter to contribute, so `Update::one` refuses to build the statement
#[test]
fn update_one_errs_on_unset_primary_key() {
    let err = Update::one(cake::ActiveModel {
        id: ActiveValue::NotSet,
        name: ActiveValue::Set("Apple Pie".to_owned()),
    })
    .expect_err("a NotSet primary key cannot narrow the update");
    assert_eq!(err, DbErr::PrimaryKeyNotSet);

    // A composite key rejects the model when any one of its columns is unset.
    let err = Update::one(cake_filling_price::ActiveModel {
        cake_id: ActiveValue::Set(1),
        filling_id: ActiveValue::NotSet,
        price: ActiveValue::Set(rust_decimal::Decimal::ONE),
    })
    .expect_err("half a composite key is not a key");
    assert_eq!(err, DbErr::PrimaryKeyNotSet);

    // `EntityTrait::update` forwards the same error.
    let err = cake::Entity::update(cake::ActiveModel {
        id: ActiveValue::NotSet,
        name: ActiveValue::Set("Apple Pie".to_owned()),
    })
    .expect_err("the entry point forwards the builder's error");
    assert_eq!(err, DbErr::PrimaryKeyNotSet);
}

// [spec:pgorm:sem:query.build.update+2/test]    `Update::many` adds no implicit
// filter; `set` writes `Set` columns including primary keys, `col_expr` writes
// a raw expression, and `QueryFilter` supplies the WHERE clause
#[test]
fn update_many_has_no_implicit_filter() {
    assert_eq!(
        Update::many(cake::Entity)
            .set(cake::ActiveModel {
                id: ActiveValue::Set(9),
                name: ActiveValue::Set("Pie".to_owned()),
            })
            .as_query()
            .to_string(QueryBuilder),
        r#"UPDATE "cake" SET "id" = 9, "name" = 'Pie'"#
    );

    assert_eq!(
        Update::many(cake::Entity)
            .set(cake::ActiveModel {
                id: ActiveValue::Unchanged(9),
                name: ActiveValue::Set("Pie".to_owned()),
            })
            .filter(cake::Column::Name.contains("Apple"))
            .as_query()
            .to_string(QueryBuilder),
        r#"UPDATE "cake" SET "name" = 'Pie' WHERE "cake"."name" LIKE '%Apple%'"#
    );

    assert_eq!(
        Update::many(fruit::Entity)
            .col_expr(fruit::Column::Name, Expr::value("Golden Apple"))
            .filter(fruit::Column::Name.contains("Apple"))
            .as_query()
            .to_string(QueryBuilder),
        r#"UPDATE "fruit" SET "name" = 'Golden Apple' WHERE "fruit"."name" LIKE '%Apple%'"#
    );
}

// [spec:pgorm:sem:query.build.delete+1/test]    `Delete::one` filters on the
// primary key only — non-key attributes never reach the WHERE clause
#[test]
fn delete_one_filters_by_primary_key_only() {
    assert_eq!(
        Delete::one(cake::Model {
            id: 1,
            name: "Apple Pie".to_owned(),
        })
        .expect("the primary key is set")
        .as_query()
        .to_string(QueryBuilder),
        r#"DELETE FROM "cake" WHERE "cake"."id" = 1"#
    );

    assert_eq!(
        Delete::one(cake::ActiveModel {
            id: ActiveValue::Unchanged(1),
            name: ActiveValue::Set("Apple Pie".to_owned()),
        })
        .expect("the primary key is unchanged, not unset")
        .as_query()
        .to_string(QueryBuilder),
        r#"DELETE FROM "cake" WHERE "cake"."id" = 1"#
    );

    assert_eq!(
        Delete::one(cake_filling::Model {
            cake_id: 1,
            filling_id: 2,
        })
        .expect("both primary-key columns are set")
        .as_query()
        .to_string(QueryBuilder),
        [
            r#"DELETE FROM "cake_filling""#,
            r#"WHERE "cake_filling"."cake_id" = 1 AND "cake_filling"."filling_id" = 2"#,
        ]
        .join(" ")
    );
}

// [spec:pgorm:sem:query.build.delete+1/test]    a `NotSet` primary key has no
// filter to contribute, so `Delete::one` refuses to build the statement
#[test]
fn delete_one_errs_on_unset_primary_key() {
    let err = Delete::one(cake::ActiveModel {
        id: ActiveValue::NotSet,
        name: ActiveValue::Set("Apple Pie".to_owned()),
    })
    .expect_err("a NotSet primary key cannot narrow the delete");
    assert_eq!(err, DbErr::PrimaryKeyNotSet);

    // A composite key rejects the model when any one of its columns is unset.
    let err = Delete::one(cake_filling::ActiveModel {
        cake_id: ActiveValue::Set(1),
        filling_id: ActiveValue::NotSet,
    })
    .expect_err("half a composite key is not a key");
    assert_eq!(err, DbErr::PrimaryKeyNotSet);

    // `EntityTrait::delete` forwards the same error.
    let err = cake::Entity::delete(cake::ActiveModel {
        id: ActiveValue::NotSet,
        name: ActiveValue::Set("Apple Pie".to_owned()),
    })
    .expect_err("the entry point forwards the builder's error");
    assert_eq!(err, DbErr::PrimaryKeyNotSet);
}

// [spec:pgorm:sem:query.build.delete+1/test]    `Delete::many` is bare; narrowing
// it is the caller's job through `QueryFilter`
#[test]
fn delete_many_is_unconstrained_until_filtered() {
    assert_eq!(
        Delete::many(fruit::Entity)
            .as_query()
            .to_string(QueryBuilder),
        r#"DELETE FROM "fruit""#
    );

    assert_eq!(
        Delete::many(fruit::Entity)
            .filter(fruit::Column::Name.contains("Apple"))
            .as_query()
            .to_string(QueryBuilder),
        r#"DELETE FROM "fruit" WHERE "fruit"."name" LIKE '%Apple%'"#
    );
}

// [spec:pgorm:def:query.build.debug-query/test]    `DebugQuery` is a plain
// holder of a `&Q` and a value — the `build` impls the two macros would target
// are commented out of the source, so the type carries no rendering method and
// raw SQL comes from `QueryTrait::build` instead
#[test]
fn debug_query_is_a_vestigial_holder() {
    let query = cake::Entity::insert(apple());
    let debug = DebugQuery {
        query: &query,
        value: 1_u8,
    };

    // Both fields are public and hold exactly what was put in them.
    assert_eq!(debug.value, 1);
    assert_eq!(
        debug.query.as_query().to_string(QueryBuilder),
        r#"INSERT INTO "cake" ("id", "name") VALUES (1, 'Apple Pie')"#
    );

    // The working replacement: the parameterised form from `QueryTrait`.
    let (sql, values) = query.build();
    assert_eq!(sql, r#"INSERT INTO "cake" ("id", "name") VALUES ($1, $2)"#);
    assert_eq!(
        values,
        Values(vec![
            Value::Int(Some(1)),
            Value::String(Some(Box::new("Apple Pie".to_owned()))),
        ])
    );
}
