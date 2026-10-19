#![allow(unused_imports, dead_code)]

//! Binary COPY as rows of pgorm `Value`s against a live server: every value
//! kind pgorm binds written and read back, and checked against the server's
//! own reading of the same values as literals; the columns the server
//! describes; the refusals of a row of the wrong width or type; models; and
//! a transaction and the metrics wrapper.

pub mod common;

pub use common::{TestContext, setup::*};

use std::ops::Bound;
use std::str::FromStr;

use futures::{StreamExt, TryStreamExt};
use pgorm::{
    ConnectionTrait, CopyColumn, CopyTrait, DatabaseConnection, Error, TransactionTrait,
    entity::prelude::*,
    metric::{InstrumentedPool, NoOpMetrics},
    pgorm_query::{
        ArrayType, Asterisk, CopyBinary, CopyFromStatement, CopySource, CopyToStatement, Expr,
        Name, Order, Query, Value,
    },
    types::Type,
};
use pretty_assertions::assert_eq;
use tokio_postgres::error::SqlState;

fn n(name: &str) -> Name {
    Name::runtime(name)
}

fn sqlstate(error: &Error) -> Option<&SqlState> {
    match error {
        Error::Postgres(error) => error.code(),
        _ => None,
    }
}

const KINDS: &str = "CREATE TABLE kinds (
    id int4 PRIMARY KEY,
    c_bool bool, c_char \"char\", c_int2 int2, c_int8 int8, c_oid oid,
    c_float4 float4, c_float8 float8, c_text text, c_varchar varchar(8), c_name name,
    c_bytea bytea, c_json json, c_jsonb jsonb, c_uuid uuid, c_date date, c_time time,
    c_timestamp timestamp, c_timestamptz timestamptz, c_numeric numeric,
    c_inet inet, c_cidr cidr, c_macaddr macaddr, c_interval interval,
    c_int4range int4range, c_numrange numrange, c_int8multirange int8multirange,
    c_tstzrange tstzrange, c_int4_array int4[], c_text_array text[],
    c_mood mood, c_mood_array mood[], c_positive positive
)";

/// The same values as `written`, as SQL literals the server parses itself.
const LITERALS: &str = "INSERT INTO kinds VALUES (3,
    true, 'A', -7, 9000000000, 4000000000,
    1.5, -2.25, 'it''s ü ☃', 'x', 'pg_class',
    '\\x0001ff', '{\"b\": 1, \"a\": [true, null]}', '{\"k\": \"v\"}',
    '67e55044-10b1-426f-9247-bb680e5fe0c8', '2026-10-19', '23:59:58.123456',
    '1999-12-31 23:00:00', to_timestamp(1700000000), 12345.6789,
    '192.168.0.1/24', '10.0.0.0/8', '00:01:02:03:04:05',
    '1 year 2 mons 3 days 00:00:04.000005',
    '[1,5)', '[1.5,)', '{[1,3),[7,9)}', 'empty', '{1,NULL,3}', '{a,\"b,c\"}',
    'loud', '{calm,loud}', 7
)";

fn int(value: i32) -> Value {
    Value::Int(Some(value))
}

fn decimal(text: &str) -> Value {
    Value::Decimal(Some(Box::new(Decimal::from_str(text).unwrap())))
}

fn range(kind: RangeType, lower: Bound<Value>, upper: Bound<Value>) -> Value {
    Value::Range(kind, Some(Box::new(Range::new(lower, upper))))
}

/// Row `id` as written: one value of each kind pgorm binds, two of them
/// (`BigUnsigned` into `int8`, `Char` into `varchar`) of a kind the column
/// reads back as another.
fn written(id: i32) -> Vec<Value> {
    vec![
        int(id),
        true.into(),
        Value::TinyInt(Some(65)),
        Value::SmallInt(Some(-7)),
        Value::BigUnsigned(Some(9_000_000_000)),
        Value::Unsigned(Some(4_000_000_000)),
        Value::Float(Some(1.5)),
        Value::Double(Some(-2.25)),
        "it's ü ☃".into(),
        Value::Char(Some('x')),
        "pg_class".into(),
        Value::Bytes(Some(Box::new(vec![0, 1, 255]))),
        Value::Json(Some(Box::new(
            serde_json::json!({"b": 1, "a": [true, null]}),
        ))),
        Value::Json(Some(Box::new(serde_json::json!({"k": "v"})))),
        Value::Uuid(Some(Box::new(
            Uuid::parse_str("67e55044-10b1-426f-9247-bb680e5fe0c8").unwrap(),
        ))),
        Value::Date(Some(Box::new(Date::constant(2026, 10, 19)))),
        Value::Time(Some(Box::new(Time::constant(23, 59, 58, 123_456_000)))),
        Value::DateTime(Some(Box::new(DateTime::constant(
            1999, 12, 31, 23, 0, 0, 0,
        )))),
        Value::DateTimeWithTimeZone(Some(Box::new(
            DateTimeWithTimeZone::from_second(1_700_000_000).unwrap(),
        ))),
        decimal("12345.6789"),
        Value::IpNetwork(Some(Box::new(
            IpNetwork::new("192.168.0.1".parse().unwrap(), 24).unwrap(),
        ))),
        Value::IpNetwork(Some(Box::new(
            IpNetwork::new("10.0.0.0".parse().unwrap(), 8).unwrap(),
        ))),
        Value::MacAddress(Some(Box::new(MacAddress::new([0, 1, 2, 3, 4, 5])))),
        Interval::new(14, 3, 4_000_005).into(),
        range(
            RangeType::Int4,
            Bound::Included(int(1)),
            Bound::Excluded(int(5)),
        ),
        range(
            RangeType::Numeric,
            Bound::Included(decimal("1.5")),
            Bound::Unbounded,
        ),
        Value::Multirange(
            RangeType::Int8,
            Some(Box::new(Multirange::from(vec![
                Range::new(
                    Bound::Included(Value::BigInt(Some(1))),
                    Bound::Excluded(Value::BigInt(Some(3))),
                ),
                Range::new(
                    Bound::Included(Value::BigInt(Some(7))),
                    Bound::Excluded(Value::BigInt(Some(9))),
                ),
            ]))),
        ),
        Value::Range(RangeType::TimestampTz, Some(Box::new(Range::Empty))),
        Value::Array(
            ArrayType::Int,
            Some(Box::new(vec![int(1), Value::Int(None), int(3)])),
        ),
        Value::Array(
            ArrayType::String,
            Some(Box::new(vec!["a".into(), "b,c".into()])),
        ),
        "loud".into(),
        Value::Array(
            ArrayType::String,
            Some(Box::new(vec!["calm".into(), "loud".into()])),
        ),
        int(7),
    ]
}

/// Row `id` as the columns' types read it back.
fn read(id: i32) -> Vec<Value> {
    let mut row = written(id);
    row[4] = Value::BigInt(Some(9_000_000_000));
    row[9] = "x".into();
    row
}

/// A row of every column NULL but its key, each NULL of its column's kind.
fn nulls(id: i32) -> Vec<Value> {
    vec![
        int(id),
        Value::Bool(None),
        Value::TinyInt(None),
        Value::SmallInt(None),
        Value::BigInt(None),
        Value::Unsigned(None),
        Value::Float(None),
        Value::Double(None),
        Value::String(None),
        Value::String(None),
        Value::String(None),
        Value::Bytes(None),
        Value::Json(None),
        Value::Json(None),
        Value::Uuid(None),
        Value::Date(None),
        Value::Time(None),
        Value::DateTime(None),
        Value::DateTimeWithTimeZone(None),
        Value::Decimal(None),
        Value::IpNetwork(None),
        Value::IpNetwork(None),
        Value::MacAddress(None),
        Value::Interval(None),
        Value::Range(RangeType::Int4, None),
        Value::Range(RangeType::Numeric, None),
        Value::Multirange(RangeType::Int8, None),
        Value::Range(RangeType::TimestampTz, None),
        Value::Array(ArrayType::Int, None),
        Value::Array(ArrayType::String, None),
        Value::String(None),
        Value::Array(ArrayType::String, None),
        Value::Int(None),
    ]
}

async fn create_kinds(db: &DatabaseConnection) -> Result<(), Error> {
    db.batch_execute("CREATE TYPE mood AS ENUM ('calm', 'loud')")
        .await?;
    db.batch_execute("CREATE DOMAIN positive AS int4 CHECK (VALUE > 0)")
        .await?;
    db.batch_execute(KINDS).await
}

/// Every row of `copy`, read through the typed stream.
async fn rows_of<C>(
    db: &mut C,
    copy: &CopyToStatement<CopyBinary>,
) -> Result<Vec<Vec<Value>>, Error>
where
    C: CopyTrait + Send,
{
    db.copy_out_rows(copy).await?.try_collect().await
}

fn ordered_kinds() -> CopyToStatement<CopyBinary> {
    CopyToStatement::new(
        Query::select()
            .column(Asterisk)
            .from(n("kinds"))
            .order_by(n("id"), Order::Asc)
            .to_owned(),
        CopyBinary,
    )
}

// [spec:pgorm:req:conn.copy.rows/test]    every value kind pgorm binds is written as binary rows and read back, as the server reads the same literals
#[pgorm_macros::test]
async fn copy_rows_round_trip_every_kind() -> Result<(), Error> {
    let ctx = TestContext::new("copy_rows_round_trip_every_kind").await;
    let mut db = ctx.db.get().await?;
    create_kinds(&db).await?;

    let mut sink = db
        .copy_in_rows(&CopyFromStatement::new(n("kinds"), CopyBinary))
        .await?;
    assert_eq!(sink.columns().len(), 33);
    assert_eq!(sink.columns()[30].name(), "c_mood");
    sink.send(written(1)).await?;
    sink.send(nulls(2)).await?;
    assert_eq!(sink.finish().await?, 2);
    db.execute(LITERALS, &[]).await?;

    // The server read what the encoder wrote as it reads the literals.
    let same = db
        .query_one(
            "SELECT (to_jsonb(a) - 'id') = (to_jsonb(b) - 'id') AS same \
             FROM kinds a, kinds b WHERE a.id = 1 AND b.id = 3",
            &[],
        )
        .await?;
    assert!(same.get::<_, bool>("same"));

    let rows = rows_of(&mut db, &ordered_kinds()).await?;
    assert_eq!(rows, vec![read(1), nulls(2), read(3)]);

    // A whole table copies as its own columns, and back in through a copy.
    db.batch_execute("CREATE TABLE kinds_copy (LIKE kinds INCLUDING ALL)")
        .await?;
    let whole = rows_of(&mut db, &CopyToStatement::new(n("kinds"), CopyBinary)).await?;
    let mut sink = db
        .copy_in_rows(&CopyFromStatement::new(n("kinds_copy"), CopyBinary))
        .await?;
    for row in whole {
        sink.send(row).await?;
    }
    assert_eq!(sink.finish().await?, 3);
    let copied = db
        .query_one(
            "SELECT count(*) AS n FROM kinds a JOIN kinds_copy b USING (id) \
             WHERE to_jsonb(a) = to_jsonb(b)",
            &[],
        )
        .await?;
    assert_eq!(copied.get::<_, i64>("n"), 3);

    drop(db);
    ctx.delete().await;
    Ok(())
}

// [spec:pgorm:req:conn.copy.rows/test]    a row of the wrong width or a value its column cannot take is refused, and the sink goes on
#[pgorm_macros::test]
async fn copy_rows_refuse_wrong_width_or_type() -> Result<(), Error> {
    let ctx = TestContext::new("copy_rows_refuse_wrong_width").await;
    let mut db = ctx.db.get().await?;
    db.batch_execute(
        "CREATE TABLE item (id int PRIMARY KEY, name text, qty int NOT NULL DEFAULT 7)",
    )
    .await?;

    let copy = CopyFromStatement::new(n("item"), CopyBinary)
        .columns([n("id"), n("name")])
        .to_owned();
    let mut sink = db.copy_in_rows(&copy).await?;
    assert_eq!(
        sink.columns(),
        [
            CopyColumn::new("id", Type::INT4),
            CopyColumn::new("name", Type::TEXT)
        ]
    );
    sink.send([int(1), "one".into()]).await?;

    let narrow = sink.send([int(2)]).await;
    assert!(
        matches!(&narrow, Err(Error::Type(message)) if message.contains("1 values for the COPY's 2 columns (id, name)")),
        "{narrow:?}"
    );
    let wide = sink.send([int(2), "two".into(), int(3)]).await;
    assert!(matches!(wide, Err(Error::Type(_))), "{wide:?}");
    let mistyped = sink.send(["two".into(), "two".into()]).await;
    assert!(
        matches!(&mistyped, Err(Error::Type(message))
            if message.contains("COPY column `id` of type `int4`") && message.contains("String")),
        "{mistyped:?}"
    );
    let decimal_into_text = sink.send([int(2), decimal("1.5")]).await;
    assert!(
        matches!(decimal_into_text, Err(Error::Type(_))),
        "{decimal_into_text:?}"
    );

    // Nothing of a refused row was written, so the COPY goes on.
    sink.send([int(2), Value::String(None)]).await?;
    assert_eq!(sink.finish().await?, 2);
    let rows = db
        .query_all("SELECT id, name, qty FROM item ORDER BY id", &[])
        .await?;
    let rows: Vec<(i32, Option<String>, i32)> = rows
        .iter()
        .map(|row| (row.get(0), row.get(1), row.get(2)))
        .collect();
    assert_eq!(rows, [(1, Some("one".to_owned()), 7), (2, None, 7)]);

    // A row the server refuses is reported by finish, as for bytes.
    let mut sink = db.copy_in_rows(&copy).await?;
    sink.send([int(1), "again".into()]).await?;
    let error = sink.finish().await.expect_err("a duplicate key");
    assert_eq!(
        sqlstate(&error),
        Some(&SqlState::UNIQUE_VIOLATION),
        "{error}"
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}

// [spec:pgorm:req:conn.copy.rows/test]    the columns come from the server: a list, a whole table without its generated columns, a query's result
#[pgorm_macros::test]
async fn copy_rows_columns_from_the_server() -> Result<(), Error> {
    let ctx = TestContext::new("copy_rows_columns_from_server").await;
    let mut db = ctx.db.get().await?;
    db.batch_execute(
        "CREATE TABLE gen (a int, b int GENERATED ALWAYS AS (a * 2) STORED, \
         c text, d int GENERATED ALWAYS AS (a + 1) VIRTUAL)",
    )
    .await?;

    let whole = [
        CopyColumn::new("a", Type::INT4),
        CopyColumn::new("c", Type::TEXT),
    ];
    let into = CopyFromStatement::new(n("gen"), CopyBinary);
    assert_eq!(db.copy_in_columns(&into).await?, whole);
    let out_of = CopyToStatement::new(n("gen"), CopyBinary);
    assert_eq!(db.copy_out_columns(&out_of).await?, whole);

    let mut sink = db.copy_in_rows(&into).await?;
    sink.send([int(4), "x".into()]).await?;
    assert_eq!(sink.finish().await?, 1);
    assert_eq!(rows_of(&mut db, &out_of).await?, [vec![int(4), "x".into()]]);
    let every = CopyToStatement::new(
        Query::select()
            .columns([n("d"), n("c"), n("b")])
            .from(n("gen"))
            .to_owned(),
        CopyBinary,
    );
    assert_eq!(
        rows_of(&mut db, &every).await?,
        [vec![int(5), "x".into(), int(8)]]
    );

    // A write is described without being run.
    let insert = Query::insert()
        .into_table(n("gen"))
        .columns([n("a")])
        .values_panic([10.into()])
        .returning(Query::returning().columns([n("b"), n("a")]))
        .to_owned();
    let returning = CopyToStatement::new(CopySource::try_from(insert)?, CopyBinary);
    assert_eq!(
        db.copy_out_columns(&returning).await?,
        [
            CopyColumn::new("b", Type::INT4),
            CopyColumn::new("a", Type::INT4)
        ]
    );
    let count = db.query_one("SELECT count(*) AS n FROM gen", &[]).await?;
    assert_eq!(count.get::<_, i64>("n"), 1);
    assert_eq!(
        rows_of(&mut db, &returning).await?,
        [vec![int(20), int(10)]]
    );

    // The server describes a domain column by its base type, whose form its
    // values travel in, and checks the domain's constraint as it reads them.
    db.batch_execute(
        "CREATE DOMAIN label AS text CHECK (VALUE <> ''); CREATE TABLE labelled (l label)",
    )
    .await?;
    let labelled = CopyFromStatement::new(n("labelled"), CopyBinary);
    assert_eq!(
        db.copy_in_columns(&labelled).await?,
        [CopyColumn::new("l", Type::TEXT)]
    );
    let mut sink = db.copy_in_rows(&labelled).await?;
    sink.send(["".into()]).await?;
    let error = sink.finish().await.expect_err("an empty label");
    assert_eq!(
        sqlstate(&error),
        Some(&SqlState::CHECK_VIOLATION),
        "{error}"
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}

/// Every answer a connection gives is its own.
async fn answers_in_step(db: &DatabaseConnection) -> Result<(), Error> {
    for value in 1..=3_i32 {
        let row = db.query_one("SELECT $1::int4 + 0", &[&value]).await?;
        assert_eq!(row.get::<_, i32>(0), value);
    }
    Ok(())
}

// [spec:pgorm:req:conn.copy.rows/test]    a description the server refuses leaves the connection as it was; a type no value holds fails its row
#[pgorm_macros::test]
async fn copy_rows_refusals_before_and_after_start() -> Result<(), Error> {
    let ctx = TestContext::new("copy_rows_refusals").await;
    let mut db = ctx.db.get().await?;

    let missing = db
        .copy_in_rows(&CopyFromStatement::new(n("missing"), CopyBinary))
        .await
        .map(drop);
    let Err(error) = missing else {
        panic!("no such table");
    };
    assert_eq!(
        sqlstate(&error),
        Some(&SqlState::UNDEFINED_TABLE),
        "{error}"
    );
    answers_in_step(&db).await?;

    let tied = Query::select()
        .expr(Expr::val(1))
        .limit_with_ties(1)
        .to_owned();
    let refused = db
        .copy_out_rows(&CopyToStatement::new(tied, CopyBinary))
        .await
        .map(drop);
    assert!(
        matches!(refused, Err(Error::QueryBuilder(_))),
        "{refused:?}"
    );

    let points = CopyToStatement::new(
        Query::select()
            .expr_as(Expr::raw("point(1, 2)"), n("spot"))
            .to_owned(),
        CopyBinary,
    );
    let mut rows = db.copy_out_rows(&points).await?;
    let first = rows.next().await;
    assert!(
        matches!(&first, Some(Err(Error::Type(message))) if message.contains("COPY column `spot` of type `point`")),
        "{first:?}"
    );
    assert!(rows.next().await.is_none());
    drop(rows);
    answers_in_step(&db).await?;

    drop(db);
    ctx.delete().await;
    Ok(())
}

mod item {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
    #[pgorm(table_name = "item")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub id: i32,
        pub name: Option<String>,
        pub qty: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

// [spec:pgorm:req:conn.copy.rows/test]    models are written and read by column name, in a transaction and through the metrics wrapper
#[pgorm_macros::test]
async fn copy_rows_models_in_a_transaction() -> Result<(), Error> {
    let ctx = TestContext::new("copy_rows_models_transaction").await;
    let pool = InstrumentedPool::new(ctx.db.clone(), NoOpMetrics);
    let mut db = pool.get().await?;
    db.batch_execute(
        "CREATE TABLE item (qty int NOT NULL, id int PRIMARY KEY, name text, note text)",
    )
    .await?;

    let models = [
        item::Model {
            id: 1,
            name: Some("apple".to_owned()),
            qty: 3,
        },
        item::Model {
            id: 2,
            name: None,
            qty: 4,
        },
    ];
    {
        let mut tx = db.begin().await?;
        let copy = CopyFromStatement::new(n("item"), CopyBinary)
            .columns([n("name"), n("id"), n("qty")])
            .to_owned();
        let mut sink = tx.copy_in_rows(&copy).await?;
        for model in &models {
            sink.send_model(model).await?;
        }
        assert_eq!(sink.finish().await?, 2);

        // The table's own column order, and a column the entity lacks.
        let read: Vec<item::Model> = tx
            .copy_out_rows(&CopyToStatement::new(n("item"), CopyBinary))
            .await?
            .models::<item::Entity>()
            .try_collect()
            .await?;
        assert_eq!(read, models);

        let mut notes = tx
            .copy_in_rows(&CopyFromStatement::new(n("item"), CopyBinary))
            .await?;
        let unknown = notes.send_model(&models[0]).await;
        assert!(
            matches!(&unknown, Err(Error::Type(message)) if message.contains("`note`")),
            "{unknown:?}"
        );
        drop(notes);
        tx.rollback().await?;
    }

    let partial = CopyToStatement::new(
        Query::select()
            .columns([n("id")])
            .from(n("item"))
            .to_owned(),
        CopyBinary,
    );
    let mut sink = db
        .copy_in_rows(
            &CopyFromStatement::new(n("item"), CopyBinary)
                .columns([n("id"), n("qty")])
                .to_owned(),
        )
        .await?;
    sink.send([int(5), int(6)]).await?;
    sink.finish().await?;
    let missing: Vec<Result<item::Model, Error>> = db
        .copy_out_rows(&partial)
        .await?
        .models::<item::Entity>()
        .collect()
        .await;
    assert!(
        matches!(missing.as_slice(), [Err(Error::AttrNotSet(_))]),
        "{missing:?}"
    );

    drop(db);
    ctx.delete().await;
    Ok(())
}

/// A model of enum and JSON fields: an enum in a schema of its own, an enum
/// reached through a domain over it, an array of an enum, and two
/// `FromJsonQueryResult` structs, one in `jsonb` and one, nullable, in `json`.
mod entry {
    use pgorm::FromJsonQueryResult;
    use pgorm::entity::prelude::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
    #[pgorm(
        rs_type = "String",
        db_type = "Enum",
        enum_name = "phase",
        schema_name = "audit"
    )]
    pub enum Phase {
        #[pgorm(string_value = "open")]
        Open,
        #[pgorm(string_value = "closed")]
        Closed,
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter, DeriveActiveEnum)]
    #[pgorm(rs_type = "String", db_type = "Enum", enum_name = "tone")]
    pub enum Tone {
        #[pgorm(string_value = "calm")]
        Calm,
        #[pgorm(string_value = "loud")]
        Loud,
    }

    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize, FromJsonQueryResult)]
    pub struct Detail {
        pub label: String,
        pub weight: f64,
        pub tags: Vec<String>,
    }

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[pgorm(schema_name = "audit", table_name = "entry")]
    pub struct Model {
        #[pgorm(primary_key, auto_increment = false)]
        pub id: i32,
        pub phase: Phase,
        pub tone: Tone,
        pub phases: Vec<Phase>,
        pub detail: Detail,
        pub extra: Option<Detail>,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

fn entries() -> Vec<entry::Model> {
    use entry::{Detail, Model, Phase, Tone};
    let detail = |label: &str, weight: f64, tags: &[&str]| Detail {
        label: label.to_owned(),
        weight,
        tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
    };
    vec![
        Model {
            id: 1,
            phase: Phase::Open,
            tone: Tone::Calm,
            phases: vec![Phase::Open, Phase::Closed],
            detail: detail("it's \"quoted\" ü", -2.5, &["a", "b,c"]),
            extra: Some(detail("", 0.0, &[])),
        },
        Model {
            id: 2,
            phase: Phase::Closed,
            tone: Tone::Loud,
            phases: Vec::new(),
            detail: detail("plain", 1e300, &["x"]),
            extra: None,
        },
    ]
}

// [spec:pgorm:req:conn.copy.rows/test]    models with enum and JSON fields: a schema's enum, an enum through a domain, an enum array, JSON structs
#[pgorm_macros::test]
async fn copy_rows_models_with_enum_and_json() -> Result<(), Error> {
    let ctx = TestContext::new("copy_rows_models_enum_json").await;
    let mut db = ctx.db.get().await?;
    db.batch_execute(
        "CREATE SCHEMA audit;
         CREATE TYPE audit.phase AS ENUM ('open', 'closed');
         CREATE TYPE tone AS ENUM ('calm', 'loud');
         CREATE DOMAIN audit.tone_d AS tone CHECK (VALUE IS NOT NULL);
         CREATE TABLE audit.entry (
             id int PRIMARY KEY, phase audit.phase NOT NULL, tone audit.tone_d NOT NULL,
             phases audit.phase[] NOT NULL, detail jsonb NOT NULL, extra json
         )",
    )
    .await?;
    let table = (n("audit"), n("entry"));

    let models = entries();
    let mut sink = db
        .copy_in_rows(&CopyFromStatement::new(table.clone(), CopyBinary))
        .await?;
    for model in &models {
        sink.send_model(model).await?;
    }
    assert_eq!(sink.finish().await?, 2);

    // The server stored what the ORM reads back, so the COPY wrote each
    // field in its column's own form.
    let found = entry::Entity::find()
        .order_by_asc(entry::Column::Id)
        .all(&db)
        .await?;
    assert_eq!(found, models);

    // The columns are described by the types the values travel in: the
    // domain by the enum it is over.
    let stream = db
        .copy_out_rows(&CopyToStatement::new(table, CopyBinary))
        .await?;
    let types: Vec<&str> = stream
        .columns()
        .iter()
        .map(|column| column.type_().name())
        .collect();
    assert_eq!(types, ["int4", "phase", "tone", "_phase", "jsonb", "json"]);
    let read: Vec<entry::Model> = stream.models::<entry::Entity>().try_collect().await?;
    assert_eq!(read, models);

    drop(db);
    ctx.delete().await;
    Ok(())
}
