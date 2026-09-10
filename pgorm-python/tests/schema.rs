//! Native DDL built independently must match Python SQL, including literal escaping.
use pgorm::pgorm_query::{
    Alias, ColumnDef, ColumnType, Expr, Index, IndexOrder, IndexType, IntoIden, StringLen, Table,
    TableName, TypeName, Values, extension::Type,
};
use pgorm_python::expressions::Compiled;
use pyo3::prelude::*;
use std::{collections::BTreeMap, ffi::CString, sync::Arc};

fn a(name: &str) -> Alias {
    Alias::new(name)
}

fn programs() -> BTreeMap<&'static str, String> {
    let table = TableName::SchemaTable(
        a("schema \"雪\"").into_iden(),
        a("items \"雪\"").into_iden(),
    );
    let kind = (a("schema \"雪\""), a("Mood \"雪\""));
    let column_type = ColumnType::Enum {
        name: kind.1.clone().into_iden(),
        schema: Some(kind.0.clone().into_iden()),
        variants: vec![],
    };
    let mut base = Table::create(table.clone());
    base.col(ColumnDef::new_with_type(a("id \"x\""), ColumnType::Integer).not_null());
    let full = base
        .clone()
        .col(
            ColumnDef::new_with_type(a("name"), ColumnType::String(StringLen::N(80)))
                .default("O'Brien \\ 雪"),
        )
        .col(
            ColumnDef::new_with_type(a("mood"), column_type.clone()).default(
                Expr::val("busy")
                    .cast_as_type(TypeName::new(kind.1.clone()).schema(kind.0.clone())),
            ),
        )
        .col(ColumnDef::new_with_type(
            a("moods"),
            ColumnType::Array(Arc::new(column_type)),
        ))
        .col(ColumnDef::new_with_type(
            a("amount"),
            ColumnType::Decimal(Some((12, 3))),
        ))
        .primary_key(&mut Index::create(table.clone(), a("id \"x\"")))
        .index(
            Index::create(table.clone(), a("name"))
                .name(a("unique \"x\""))
                .unique()
                .nulls_not_distinct(),
        )
        .check(Expr::col(a("id \"x\"")).gt(0i64))
        .if_not_exists()
        .to_string();
    BTreeMap::from([
        ("table", full),
        (
            "generated",
            base.col(
                ColumnDef::new_with_type(a("twice"), ColumnType::Integer)
                    .generated(Expr::col(a("id \"x\"")).mul(2i64), true),
            )
            .to_string(),
        ),
        (
            "column_specs",
            Table::create(table.clone())
                .col(
                    ColumnDef::new_with_type(a("id"), ColumnType::BigInteger)
                        .primary_key()
                        .auto_increment(),
                )
                .col(
                    ColumnDef::new_with_type(a("n"), ColumnType::Integer)
                        .unique_key()
                        .null()
                        .check(Expr::col(a("n")).gt(0i64)),
                )
                .to_string(),
        ),
        (
            "index",
            Index::create(table.clone(), a("name"))
                .name(a("index \"x\""))
                .col((a("id \"x\""), IndexOrder::Desc))
                .unique()
                .nulls_not_distinct()
                .index_type(IndexType::BTree)
                .if_not_exists()
                .to_string(),
        ),
        (
            "index_gin",
            Index::create(table.clone(), a("moods"))
                .index_type(IndexType::Custom(a("gin").into_iden()))
                .to_string(),
        ),
        (
            "drop_index",
            Index::drop(a("index \"x\""))
                .table(table.clone())
                .if_exists()
                .to_string(),
        ),
        (
            "drop_table",
            Table::drop(table.clone()).if_exists().cascade().to_string(),
        ),
        (
            "rename_table",
            Table::rename(table.clone(), a("new \"x\"")).to_string(),
        ),
        (
            "rename_column",
            Table::rename_column(table.clone(), a("id \"x\""), a("new \"x\"")).to_string(),
        ),
        ("truncate", Table::truncate(table.clone()).to_string()),
        (
            "add_column",
            Table::alter(table.clone())
                .add_column_if_not_exists(ColumnDef::new_with_type(a("extra"), ColumnType::Text))
                .to_string(),
        ),
        (
            "modify_column",
            Table::alter(table.clone())
                .modify_column(
                    ColumnDef::new_with_type(a("extra"), ColumnType::String(StringLen::None))
                        .not_null()
                        .default("hello"),
                )
                .to_string(),
        ),
        (
            "drop_column",
            Table::alter(table).drop_column(a("extra")).to_string(),
        ),
        (
            "enum",
            Type::create(kind.clone())
                .as_enum()
                .values([a(""), a("O'Brien \\ 雪"), a("busy")])
                .to_string(),
        ),
        (
            "enum_before",
            Type::alter(kind.clone())
                .add_value(a("new"))
                .before(a("busy"))
                .to_string(),
        ),
        (
            "enum_after",
            Type::alter(kind.clone())
                .add_value(a("new"))
                .after(a("busy"))
                .to_string(),
        ),
        (
            "enum_rename_value",
            Type::alter(kind.clone())
                .rename_value(a("busy"), a("calm"))
                .to_string(),
        ),
        (
            "enum_rename",
            Type::alter(kind.clone())
                .rename_to(a("new \"x\""))
                .to_string(),
        ),
        (
            "enum_drop",
            Type::drop(kind).if_exists().cascade().to_string(),
        ),
    ])
}

// [spec:pgorm:req:python.schema/test]
#[test]
fn ddl_matches_native_rust() -> Result<(), Box<dyn std::error::Error>> {
    Python::initialize();
    Python::attach(|py| -> Result<(), Box<dyn std::error::Error>> {
        let native = PyModule::new(py, "pgorm._native")?;
        pgorm_python::install(&native, Default::default())?;
        let source = CString::new(include_str!("schema_programs.py"))?;
        let examples = PyModule::from_code(py, &source, c"schema_programs.py", c"schema_programs")?;
        let queries = examples.call_method1("programs", (&native,))?;
        let rust = programs();
        assert_eq!(queries.len()?, rust.len());
        for (name, sql) in rust {
            let query = queries.get_item(name)?;
            let compiled: Compiled = query
                .call_method0("inspect")?
                .extract()
                .map_err(PyErr::from)?;
            assert_eq!(compiled.sql, sql, "{name}");
            assert_eq!(compiled.values, Values(vec![]), "{name}");
            let executable = pgorm_python::statements::compile(&query)?;
            assert_eq!(
                (executable.sql, executable.values),
                (compiled.sql, compiled.values)
            );
        }
        Ok(())
    })
}
