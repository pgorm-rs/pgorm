//! Parity: each case of `tests/parity.json`, built here with the
//! application's entities through pgorm's own API, is the SQL and values the
//! JavaScript suite's `inspect()` gives for the same case through the
//! binding, which holds itself to the same file in both runtimes.

use std::collections::BTreeMap;

use pgorm::pgorm_query::{Expr, Name, Values};
use pgorm::{
    ColumnTrait, EntityTrait, QueryFilter, QueryOrder, QuerySelect, QueryTrait, pgorm_query::Value,
};
use serde_json::Value as Json;

use crate::{account, graphs, membership};

fn built<Q: QueryTrait>(query: &Q) -> (String, Values)
where
    Q::QueryStatement: pgorm::pgorm_query::QueryStatementBuilder,
{
    use pgorm::pgorm_query::QueryStatementBuilder;
    query.as_query().build()
}

/// A value as the golden file writes it: its kind and its text.
fn canonical(value: &Value) -> [String; 2] {
    let (kind, text) = match value {
        Value::Int(Some(v)) => ("i32", v.to_string()),
        Value::BigUnsigned(Some(v)) => ("u64", v.to_string()),
        Value::String(Some(v)) => ("text", v.to_string()),
        other => panic!("no canonical text for {other:?}"),
    };
    [kind.to_owned(), text]
}

fn aliases(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| (*name).to_owned()).collect()
}

fn cases() -> Vec<(&'static str, (String, Values))> {
    vec![
        ("find", built(&account::Entity::find())),
        (
            "find-filtered",
            built(
                &account::Entity::find()
                    .filter(account::Column::Id.gte(10))
                    .order_by_desc(account::Column::Name)
                    .limit(20),
            ),
        ),
        (
            "find-enum",
            built(&account::Entity::find().filter(account::Column::Mood.eq(account::Mood::Busy))),
        ),
        ("find-one", built(&account::Entity::find().limit(1))),
        (
            "find-composite",
            built(
                &membership::Entity::find()
                    .filter(membership::Column::AccountId.eq(1))
                    .filter(membership::Column::Team.eq("red")),
            ),
        ),
        ("graph-notes", built(&graphs::optional(&aliases(&["n"])))),
        (
            "graph-mixed-default-aliases",
            built(
                &graphs::required(&aliases(&["g1"])).join_maybe_as::<crate::note::Entity>(
                    pgorm::RelationTrait::def(&account::Relation::Note),
                    Name::runtime("g2"),
                ),
            ),
        ),
        (
            "graph-filtered",
            built(
                &graphs::optional(&aliases(&["n"]))
                    .filter(Expr::col((Name::runtime("n"), Name::runtime("body"))).eq("x"))
                    .order_by_asc(account::Column::Id),
            ),
        ),
    ]
}

// [spec:pgorm:req:napi.entity-reads/test]
// [spec:pgorm:req:napi.entity-graphs/test]
#[test]
fn entity_statements_match_the_golden_file() {
    let golden: BTreeMap<String, Json> = serde_json::from_str(include_str!("../tests/parity.json"))
        .expect("the golden file is JSON");
    let cases = cases();
    let mut names: Vec<&str> = cases.iter().map(|(name, _)| *name).collect();
    names.sort_unstable();
    assert_eq!(names, golden.keys().map(String::as_str).collect::<Vec<_>>());
    for (name, (sql, values)) in cases {
        assert_eq!(
            golden[name]["sql"].as_str(),
            Some(sql.as_str()),
            "{name}: SQL"
        );
        let values: Vec<[String; 2]> = values.0.iter().map(canonical).collect();
        let expected: Vec<[String; 2]> =
            serde_json::from_value(golden[name]["values"].clone()).expect("values are pairs");
        assert_eq!(values, expected, "{name}: values");
    }
}
