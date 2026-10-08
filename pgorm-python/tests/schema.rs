//! Native DDL built independently must match Python SQL, including literal escaping.
use pgorm::pgorm_query::{
    Check, ColumnDef, ColumnType, ConstraintChange, ConstraintDrop, Enforcement, Expr,
    GeneratedKind, Index, IndexOrder, IndexType, IntoName, Name, NotNullConstraint, StringLen,
    Table, TableKey, TableName, TypeName, Values, extension::Type,
};
use pgorm_python::expressions::Compiled;
use pyo3::prelude::*;
use std::{collections::BTreeMap, ffi::CString, sync::Arc};

fn a(name: &str) -> Name {
    Name::runtime(name)
}

fn programs() -> BTreeMap<&'static str, String> {
    let table = TableName::SchemaTable(
        a("schema \"雪\"").into_name(),
        a("items \"雪\"").into_name(),
    );
    let kind = (a("schema \"雪\""), a("Mood \"雪\""));
    let column_type = ColumnType::Enum {
        name: kind.1.clone().into_name(),
        schema: Some(kind.0.clone().into_name()),
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
        .primary_key(a("id \"x\""))
        .unique(
            TableKey::new(a("name"))
                .nulls_not_distinct()
                .name(a("unique \"x\"")),
        )
        .check(Expr::col(a("id \"x\"")).gt(0i64))
        .if_not_exists()
        .to_string();
    BTreeMap::from([
        ("table", full),
        (
            "generated",
            base.clone()
                .col(
                    ColumnDef::new_with_type(a("twice"), ColumnType::Integer)
                        .generated(Expr::col(a("id \"x\"")).mul(2i64), GeneratedKind::Stored),
                )
                .to_string(),
        ),
        (
            "generated_virtual",
            base.col(
                ColumnDef::new_with_type(a("next"), ColumnType::Integer)
                    .generated(Expr::col(a("id \"x\"")).add(1i64), GeneratedKind::Virtual),
            )
            .to_string(),
        ),
        (
            "column_specs",
            Table::create(table.clone())
                .col(ColumnDef::new_with_type(a("id"), ColumnType::BigInteger).auto_increment())
                .col(
                    ColumnDef::new_with_type(a("n"), ColumnType::Integer)
                        .null()
                        .check(Expr::col(a("n")).gt(0i64)),
                )
                .primary_key(a("id"))
                .unique(a("n"))
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
                .index_type(IndexType::Named(a("gin").into_name()))
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
            "add_primary_key",
            Table::alter(table.clone())
                .add_primary_key((a("id \"x\""), a("name")))
                .to_string(),
        ),
        (
            "add_unique",
            Table::alter(table.clone())
                .add_unique(
                    TableKey::new(a("name"))
                        .col(a("id \"x\""))
                        .name(a("unique \"x\""))
                        .nulls_not_distinct(),
                )
                .to_string(),
        ),
        (
            "temporal_keys",
            Table::create(table.clone())
                .col(ColumnDef::new_with_type(a("id \"x\""), ColumnType::Integer).not_null())
                .col(ColumnDef::new_with_type(
                    a("during \"x\""),
                    ColumnType::Enum {
                        name: a("tstzrange").into_name(),
                        schema: None,
                        variants: vec![],
                    },
                ))
                .primary_key(TableKey::new(a("id \"x\"")).without_overlaps(a("during \"x\"")))
                .unique(
                    TableKey::new(a("name"))
                        .col(a("id \"x\""))
                        .name(a("unique \"x\""))
                        .nulls_not_distinct()
                        .without_overlaps(a("during \"x\"")),
                )
                .to_string(),
        ),
        (
            "add_primary_key_temporal",
            Table::alter(table.clone())
                .add_primary_key(TableKey::new(a("id \"x\"")).without_overlaps(a("during \"x\"")))
                .to_string(),
        ),
        (
            "add_unique_temporal",
            Table::alter(table.clone())
                .add_unique(TableKey::new(a("name")).without_overlaps(a("during \"x\"")))
                .to_string(),
        ),
        (
            "drop_column",
            Table::alter(table.clone())
                .drop_column(a("extra"))
                .to_string(),
        ),
        (
            "set_expression",
            Table::alter(table.clone())
                .set_expression(a("twice"), Expr::col(a("id \"x\"")).mul(3i64))
                .to_string(),
        ),
        (
            "drop_expression",
            Table::alter(table.clone())
                .drop_expression(a("twice"))
                .to_string(),
        ),
        (
            "drop_expression_if_exists",
            Table::alter(table.clone())
                .drop_expression_if_exists(a("twice"))
                .to_string(),
        ),
        (
            "not_null_named",
            Table::create(table.clone())
                .col(
                    ColumnDef::new_with_type(a("extra"), ColumnType::Text)
                        .not_null_named(a("present \"x\""))
                        .not_null_no_inherit(),
                )
                .col(ColumnDef::new_with_type(a("kept"), ColumnType::Text).not_null_no_inherit())
                .to_string(),
        ),
        (
            "add_not_null",
            Table::alter(table.clone())
                .add_not_null(NotNullConstraint::new(a("extra")))
                .to_string(),
        ),
        (
            "add_not_null_named",
            Table::alter(table.clone())
                .add_not_null(
                    NotNullConstraint::new(a("extra"))
                        .name(a("present \"x\""))
                        .no_inherit()
                        .not_valid(),
                )
                .to_string(),
        ),
        (
            "validate_constraint",
            Table::alter(table.clone())
                .validate_constraint(a("present \"x\""))
                .to_string(),
        ),
        (
            "alter_constraint_inherit",
            Table::alter(table.clone())
                .alter_constraint(a("present \"x\""), ConstraintChange::Inherit)
                .to_string(),
        ),
        (
            "alter_constraint_no_inherit",
            Table::alter(table.clone())
                .alter_constraint(a("present \"x\""), ConstraintChange::NoInherit)
                .to_string(),
        ),
        (
            "drop_constraint",
            Table::alter(table.clone())
                .drop_constraint(a("present \"x\""))
                .to_string(),
        ),
        (
            "drop_constraint_if_exists",
            Table::alter(table.clone())
                .drop_constraint(
                    ConstraintDrop::new(a("present \"x\""))
                        .if_exists()
                        .cascade(),
                )
                .to_string(),
        ),
        (
            "rename_constraint",
            Table::rename_constraint(table.clone(), a("present \"x\""), a("kept \"x\""))
                .to_string(),
        ),
        (
            "check_named",
            Table::create(table.clone())
                .col(
                    ColumnDef::new_with_type(a("n"), ColumnType::Integer).check(
                        Check::new(Expr::col(a("n")).gt(0i64))
                            .name(a("positive \"x\""))
                            .enforcement(Enforcement::NotEnforced),
                    ),
                )
                .check(Check::new(Expr::col(a("n")).lt(100i64)).name(a("small \"x\"")))
                .check(Check::new(Expr::col(a("n")).ne(7i64)).enforcement(Enforcement::NotEnforced))
                .to_string(),
        ),
        (
            "add_check",
            Table::alter(table.clone())
                .add_check(
                    Check::new(Expr::col(a("n")).gt(0i64))
                        .name(a("positive \"x\""))
                        .enforcement(Enforcement::NotEnforced),
                )
                .to_string(),
        ),
        (
            "add_check_plain",
            Table::alter(table.clone())
                .add_check(Expr::col(a("n")).gt(0i64))
                .to_string(),
        ),
        (
            "alter_constraint_enforced",
            Table::alter(table.clone())
                .alter_constraint(a("fk \"x\""), ConstraintChange::Enforced)
                .to_string(),
        ),
        (
            "alter_constraint_not_enforced",
            Table::alter(table)
                .alter_constraint(a("fk \"x\""), ConstraintChange::NotEnforced)
                .to_string(),
        ),
        (
            "enum",
            Type::create(kind.clone())
                .as_enum()
                .values(["", "O'Brien \\ 雪", "busy"])
                .to_string(),
        ),
        (
            "enum_before",
            Type::alter(kind.clone())
                .add_value("new")
                .before("busy")
                .to_string(),
        ),
        (
            "enum_after",
            Type::alter(kind.clone())
                .add_value("new")
                .after("busy")
                .to_string(),
        ),
        (
            "enum_rename_value",
            Type::alter(kind.clone())
                .rename_value("busy", "calm")
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
