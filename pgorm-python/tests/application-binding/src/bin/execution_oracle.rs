//! Compare installed Python execution with independent Rust operations.
use _native::{account, graphs, note};
use futures_util::TryStreamExt;
use pgorm::pgorm_query::{Alias, ColumnDef, ColumnType, Expr, Order, Query, Table};
use pgorm::pipeline::{self as pl, ExprOps, IntoSource, JoinSide};
use pgorm::{
    ActiveModelBehavior, ActiveModelTrait, ColumnTrait, ConnectionTrait, DatabaseConnection,
    EntityTrait, ModelTrait, QueryFilter, QueryOrder, Schema, TransactionMode, TransactionTrait,
    ValueHolder, Values, set,
};
use serde_json::{Value, json};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn parameters(values: Values) -> Vec<ValueHolder> {
    values.0.into_iter().map(ValueHolder).collect()
}

async fn execute<C: ConnectionTrait>(db: &C, query: (String, Values)) -> Result<u64> {
    let values = parameters(query.1);
    Ok(db.execute_raw(&query.0, values.iter()).await?)
}

async fn runtime<C: ConnectionTrait>(db: &C) -> Result<Value> {
    let table = ("python_entities", "runtime");
    let ddl = Table::create(table)
        .col(ColumnDef::new_with_type("id", ColumnType::Integer).primary_key())
        .col(ColumnDef::new_with_type("name", ColumnType::Text))
        .to_string();
    db.execute(&ddl, &[]).await?;
    let inserted = execute(
        db,
        Query::insert()
            .into_table(table)
            .columns(["id", "name"])
            .values([Expr::val(1i64).into(), Expr::val("O'Brien 雪").into()])?
            .build(),
    )
    .await?;
    let (sql, values) = Query::update()
        .table(table)
        .value("name", "changed")
        .and_where(Expr::col("id").eq(1i64))
        .returning(Query::returning().columns(["id", "name"]))
        .build();
    let values = parameters(values);
    let rows = db
        .query_raw(&sql, values.iter())
        .await?
        .try_collect::<Vec<_>>()
        .await?;
    assert_eq!(rows.len(), 1);
    let row = &rows[0];
    let changed = (row.try_get::<_, i32>(0)?, row.try_get::<_, String>(1)?);
    let raw = db
        .query_one("SELECT $1::text AS value", &[&"raw 雪"])
        .await?
        .try_get::<_, String>(0)?;
    let deleted = execute(
        db,
        Query::delete()
            .from_table(table)
            .and_where(Expr::col("id").eq(1i64))
            .build(),
    )
    .await?;
    let empty = db
        .query_opt("SELECT id FROM python_entities.runtime", &[])
        .await?
        .is_none();
    db.execute(&Table::drop(table).to_string(), &[]).await?;
    Ok(
        json!({"inserted": inserted, "changed": changed, "raw": raw, "deleted": deleted, "missing": empty}),
    )
}

fn active(id: i32, name: &str) -> account::ActiveModel {
    account::ActiveModel {
        id: set(id),
        name: set(name.to_owned()),
        note: set(None),
        ..account::ActiveModel::new()
    }
}

fn model(row: account::Model) -> Value {
    json!({"id": row.id, "name": row.name, "note": row.note, "version": row.version})
}

async fn models<C: ConnectionTrait>(db: &C) -> Result<Value> {
    db.execute(
        "TRUNCATE python_entities.notes, python_entities.accounts",
        &[],
    )
    .await?;
    let inserted = active(1, "Native").insert(db).await?;
    active(2, "Other").insert(db).await?;
    note::ActiveModel {
        id: set(11),
        account_id: set(1),
        body: set("note 雪".to_owned()),
    }
    .insert(db)
    .await?;
    let query = account::Entity::find().order_by_asc(account::Column::Id);
    let all: Vec<_> = query
        .clone()
        .all(db)
        .await?
        .into_iter()
        .map(model)
        .collect();
    let one = model(query.clone().one(db).await?);
    let missing = query
        .filter(ColumnTrait::eq(&account::Column::Id, 99))
        .one_opt(db)
        .await?
        .is_none();
    let graph = graphs::optional(&["n".to_owned()])
        .order_by_asc(Expr::col((Alias::new("accounts"), account::Column::Id)));
    let joined: Vec<_> = graph
        .clone()
        .all(db)
        .await?
        .into_iter()
        .map(|(a, n)| (a.id, n.map(|row| row.id)))
        .collect();
    let mut cursor = graph.clone().cursor_by(account::Column::Id);
    let cursor_rows: Vec<_> = cursor
        .first(2)
        .all(db)
        .await?
        .into_iter()
        .map(|(a, n)| (a.id, n.map(|row| row.id)))
        .collect();
    let graph_first = graph
        .clone()
        .one_opt(db)
        .await?
        .ok_or("expected a graph row")?
        .0
        .id;
    let graph_missing = graph
        .filter(Expr::col(("accounts", "id")).eq(99i64))
        .one_opt(db)
        .await?
        .is_none();
    let pipeline = pl::Pipeline::from(account::Entity)
        .join(
            JoinSide::Left,
            note::Entity.named("n"),
            pl::col(Alias::new("accounts"), Alias::new("id"))
                .eq(pl::col(Alias::new("n"), Alias::new("account_id"))),
        )
        .sort(pl::col(Alias::new("accounts"), Alias::new("id")));
    let sources: Vec<_> = pipeline
        .clone()
        .select_sources((account::Entity, note::Entity.named("n")))
        .all(db)
        .await?
        .into_iter()
        .map(|(a, n)| (a.map(|row| row.id), n.map(|row| row.id)))
        .collect();
    let source_one = pipeline
        .clone()
        .select_sources((account::Entity, note::Entity.named("n")))
        .one(db)
        .await?
        .0
        .map(|row| row.id);
    let source_missing = pipeline
        .filter(false)
        .select_sources((account::Entity, note::Entity.named("n")))
        .one_opt(db)
        .await?
        .is_none();
    let plain: Vec<i32> = pl::Pipeline::from(account::Entity)
        .select(pl::col(Alias::new("accounts"), Alias::new("id")))
        .sort(pl::col(Alias::new("accounts"), Alias::new("id")))
        .into_tuple::<i32>()?
        .all(db)
        .await?;
    let mut changed = inserted.clone().into_active();
    changed.name = set("changed".to_owned());
    let changed = changed.update(db).await?;
    let changed_report = model(changed.clone());
    let deleted = changed.into_active().delete(db).await?;
    Ok(
        json!({"inserted": model(inserted), "all": all, "one": one, "missing": missing,
              "graph": joined, "cursor": cursor_rows, "graph_first": graph_first, "graph_missing": graph_missing,
              "sources": sources, "source_one": source_one, "source_missing": source_missing,
              "pipeline": plain, "updated": changed_report, "deleted": deleted}),
    )
}

async fn exercise<C: ConnectionTrait>(db: &C) -> Result<Value> {
    Ok(json!({"runtime": runtime(db).await?, "models": models(db).await?}))
}

async fn compare(db: &mut DatabaseConnection, report: &Value) -> Result {
    let schema = Schema::new();
    for statement in schema.create_enum_from_entity(account::Entity) {
        db.execute(&statement.to_string(), &[]).await?;
    }
    db.execute(
        &schema.create_table_from_entity(account::Entity).to_string(),
        &[],
    )
    .await?;
    db.execute(
        &schema.create_table_from_entity(note::Entity).to_string(),
        &[],
    )
    .await?;
    let connection = exercise(db).await?;
    let mut transaction = db.begin().await?;
    let transaction_report = exercise(&transaction).await?;
    let child = transaction.begin().await?;
    active(3, "savepoint").insert(&child).await?;
    child.rollback().await?;
    let savepoint = account::Entity::find_by_id(3)
        .one_opt(&transaction)
        .await?
        .is_none();
    transaction.commit().await?;
    let transaction = db.begin().await?;
    active(4, "rolled back").insert(&transaction).await?;
    transaction.rollback().await?;
    let rollback = account::Entity::find_by_id(4).one_opt(db).await?.is_none();
    let read_only = db
        .begin_with(TransactionMode::ReadOnly { isolation: None })
        .await?;
    let mode = read_only
        .query_one("SHOW transaction_read_only", &[])
        .await?
        .try_get::<_, String>(0)?
        == "on";
    read_only.rollback().await?;
    let (sql, values) = Query::select()
        .column("id")
        .from(("python_entities", "accounts"))
        .order_by("id", Order::Asc)
        .build();
    let values = parameters(values);
    let stream = db.query_raw(&sql, values.iter()).await?;
    let streamed: Vec<_> = stream
        .try_collect::<Vec<_>>()
        .await?
        .into_iter()
        .map(|row| row.try_get::<_, i32>(0))
        .collect::<std::result::Result<_, _>>()?;
    let actual = json!({"connection": connection, "transaction": transaction_report,
                       "savepoint": savepoint, "rollback": rollback, "read_only": mode, "stream": streamed});
    if report != &actual {
        return Err(format!("Python execution {report} differs from native Rust {actual}").into());
    }
    Ok(())
}

// [spec:pgorm:req:python.acceptance+1/test]
async fn run() -> Result {
    let report: Value =
        serde_json::from_slice(&std::fs::read(std::env::var("PGORM_EXECUTION_REPORT")?)?)?;
    let pool = pgorm::connect(std::env::var("PGORM_TEST_DSN")?.parse()?);
    let mut db = pool.get().await?;
    db.execute("CREATE SCHEMA python_entities", &[]).await?;
    let result = compare(&mut db, &report).await;
    let cleanup = db.execute("DROP SCHEMA python_entities CASCADE", &[]).await;
    result?;
    cleanup?;
    println!(
        "Installed Python connection, transaction, savepoint and stream outcomes match native Rust"
    );
    Ok(())
}

fn main() -> Result {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(run())
}
