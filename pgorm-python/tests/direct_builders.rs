//! Independent Rust builders verify the installed Python suite's exact SQL/Values.

use std::collections::BTreeMap;

use pgorm::pgorm_query::{
    Alias, Condition, Expr, IntoNamedTable, JoinType, NullOrdering, Order, Query, SimpleExpr,
    Value, Values,
};
use pgorm_python::values::PyValue;
use pyo3::prelude::*;
use serde_json::{Value as Json, json};

type TestResult<T = ()> = Result<T, Box<dyn std::error::Error>>;
type Programs = BTreeMap<&'static str, (String, Values)>;

fn text<'a>(input: &'a Json, name: &str) -> TestResult<&'a str> {
    input[name]
        .as_str()
        .ok_or_else(|| format!("missing string {name}").into())
}

fn operand(value: impl Into<Value>, literal: bool) -> SimpleExpr {
    if literal {
        SimpleExpr::Constant(value.into())
    } else {
        Expr::value(value)
    }
}

fn programs(report: &Json, run: &Json) -> TestResult<Programs> {
    let schema = text(report, "schema")?;
    let accounts = (Alias::new(schema), Alias::new(text(report, "accounts")?)).into_named_table();
    let events = (Alias::new(schema), Alias::new(text(report, "events")?)).into_named_table();
    let literal = text(run, "mode")? == "literal";
    let variant = run["variant"].as_i64().ok_or("missing variant")?;
    let term = text(run, "term")?;
    let threshold = run["threshold"].as_i64().ok_or("missing threshold")?;
    let mut programs = Programs::new();

    let mut insert = Query::insert();
    insert
        .into_table(accounts.clone())
        .columns(["id", "name", "active", "visits"]);
    for (id, name, visits) in [
        (1i64, "Nora O'Brien", 10i64),
        (2, "Beta%_\\雪", 20),
        (3, "No event", 30),
    ] {
        insert.values([
            operand(id, literal),
            operand(name, literal),
            operand(true, literal),
            operand(visits, literal),
        ])?;
    }
    programs.insert("insert_accounts", insert.build());
    let mut insert = Query::insert();
    insert
        .into_table(events.clone())
        .columns(["id", "account_id", "kind", "points"]);
    for (id, account, kind, points) in [
        (10i64, 1i64, term, 2i64),
        (11, 1, "other", 3),
        (12, 2, term, 4),
    ] {
        insert.values([
            operand(id, literal),
            operand(account, literal),
            operand(kind, literal),
            operand(points, literal),
        ])?;
    }
    programs.insert("insert_events", insert.build());

    let mut select = Query::select();
    select
        .expr_as(Expr::col(("a", "id")), "account_id")
        .column(("a", "name"))
        .expr_as(Expr::col(("e", "id")), "event_id")
        .column(("e", "kind"))
        .from(accounts.clone().alias("a"))
        .join(
            JoinType::LeftJoin,
            events.clone().alias("e"),
            Expr::col(("a", "id")).equals(("e", "account_id")),
        );
    let mut predicate = Condition::all()
        .add(Expr::col(("a", "active")).eq(operand(true, literal)))
        .add(
            Condition::any()
                .add(Expr::col(("e", "kind")).eq(operand(term, literal)))
                .add(Expr::col(("e", "id")).is_null()),
        )
        .add(Expr::col(("a", "visits")).gte(operand(threshold, literal)));
    if variant != 0 {
        predicate = predicate.add(Expr::col(("a", "id")).gte(operand(2i64, literal)));
    }
    select
        .cond_where(predicate)
        .order_by(("a", "id"), Order::Asc)
        .order_by_with_nulls(("e", "id"), Order::Asc, NullOrdering::Last);
    if variant != 0 {
        select.limit(2);
    }
    programs.insert("select_join", select.build());

    programs.insert(
        "update_returning",
        Query::update()
            .table(accounts.clone())
            .value(
                "visits",
                Expr::col("visits").add(operand(5 + variant, literal)),
            )
            .and_where(Expr::col("id").eq(operand(1i64, literal)))
            .returning(Query::returning().columns(["id", "name", "visits"]))
            .build(),
    );
    programs.insert(
        "delete_event",
        Query::delete()
            .from_table(events)
            .and_where(Expr::col("id").eq(operand(11i64, literal)))
            .build(),
    );
    programs.insert(
        "select_missing",
        Query::select()
            .column("id")
            .from(accounts.clone())
            .and_where(Expr::col("id").eq(operand(999i64, literal)))
            .build(),
    );
    programs.insert(
        "select_final",
        Query::select()
            .columns(["id", "visits"])
            .from(accounts)
            .order_by("id", Order::Asc)
            .build(),
    );
    Ok(programs)
}

fn snapshots(values: Values) -> TestResult<Json> {
    Python::attach(|py| {
        let json = py.import("json")?;
        let mut result = Vec::new();
        for value in values.0 {
            // Only the stable snapshot encoder is shared. Expected query ASTs
            // and their parameter values above are built independently in Rust.
            let value = Py::new(py, PyValue::from_rust(value))?;
            let snapshot = value.bind(py).call_method0("snapshot")?;
            let encoded: String = json.call_method1("dumps", (snapshot,))?.extract()?;
            result.push(serde_json::from_str::<Json>(&encoded)?);
        }
        Ok(Json::Array(result))
    })
}

// [spec:pgorm:req:python.direct-builders/test]
#[test]
fn installed_query_evidence_matches_rust_builders() -> TestResult {
    Python::initialize();
    let report = std::env::var("PGORM_DIRECT_REPORT")?;
    let report: Json = serde_json::from_str(&std::fs::read_to_string(report)?)?;
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["transport"], "in-process");
    let runs = report["runs"].as_array().ok_or("missing runs")?;
    assert_eq!(runs.len(), 4);
    for (index, run) in runs.iter().enumerate() {
        assert_eq!(run["mode"], if index < 2 { "bound" } else { "literal" });
        let variant = index % 2;
        assert_eq!(run["variant"], variant);
        assert_eq!(run["threshold"], if variant == 0 { 0 } else { 15 });
        assert_eq!(
            run["verified"],
            json!({"inserted_accounts":3,"inserted_events":3,"selected":3-variant,"updated_visits":15+variant,"deleted":[1,0],"missing":true})
        );
        let expected = programs(&report, run)?;
        assert_eq!(
            run["cases"].as_object().ok_or("missing cases")?.len(),
            expected.len()
        );
        for (name, (sql, values)) in expected {
            assert_eq!(
                run["cases"][name]["sql"], sql,
                "SQL for {} {variant} {name}",
                run["mode"]
            );
            assert_eq!(
                run["cases"][name]["params"],
                snapshots(values)?,
                "parameters for {} {variant} {name}",
                run["mode"]
            );
        }
    }
    Ok(())
}
