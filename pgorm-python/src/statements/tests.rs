use std::ffi::CString;

use pgorm::pgorm_query::{
    Condition, Expr, Func, IntoNamedTable, JoinType, NullOrdering, OnConflict, Order, Query,
    SimpleExpr, Values,
};
use pyo3::{prelude::*, types::PyDict};

use crate::expressions::Compiled;

fn module(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let module = PyModule::new(py, "pgorm")?;
    crate::values::register(&module)?;
    crate::identifiers::register(&module)?;
    crate::expressions::register(&module)?;
    super::register(&module)?;
    let globals = PyDict::new(py);
    globals.set_item("p", module)?;
    Ok(globals)
}

fn parity(
    py: Python<'_>,
    globals: &Bound<'_, PyDict>,
    source: &str,
    expected: (String, Values),
) -> PyResult<()> {
    let object = py.eval(&CString::new(source)?, Some(globals), None)?;
    let inspected = object.call_method0("inspect")?;
    let inspected = inspected.extract::<PyRef<'_, Compiled>>()?;
    assert_eq!(inspected.sql, expected.0);
    assert_eq!(inspected.values, expected.1);
    let executable = super::compile(&object)?;
    assert_eq!(executable.sql, inspected.sql);
    assert_eq!(executable.values, inspected.values);
    Ok(())
}

// [spec:pgorm:req:python.statements/test]
#[test]
fn select_structure_matches_rust_builders() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let a = ("application", "account").into_named_table().alias("a");
        let e = ("application", "event").into_named_table().alias("e");
        let count: SimpleExpr = Func::count(Expr::col(("e", "id"))).into();
        let expected = Query::select()
            .column(("a", "name"))
            .expr_as(count.clone(), "events")
            .from(a)
            .join(
                JoinType::LeftJoin,
                e,
                Expr::col(("a", "id")).equals(("e", "account_id")),
            )
            .cond_where(
                Condition::all()
                    .add(Expr::col(("a", "active")).eq(true))
                    .add(
                        Condition::any()
                            .add(Expr::col(("e", "kind")).eq("click"))
                            .add(Expr::col(("e", "id")).is_null()),
                    ),
            )
            .add_group_by([Expr::col(("a", "name")).into()])
            .cond_having(Expr::expr(count.clone()).gt(2i64))
            .order_by_expr_with_nulls(count, Order::Desc, NullOrdering::Last)
            .order_by(("a", "name"), Order::Asc)
            .limit(5)
            .offset(1)
            .build();
        py.run(c"a = p.Table('account', schema='application', alias='a')\ne = p.Table('event', schema='application', alias='e')\nn = p.call('count', e.col('id'))", Some(&globals), None)?;
        parity(
            py,
            &globals,
            "p.Select(a.col('name'), n.as_('events')).from_(a).join(e, a.col('id') == e.col('account_id'), kind=p.Join.Left).where_(p.Condition.all(a.col('active') == True, p.Condition.any(e.col('kind') == 'click', e.col('id').is_null()))).group_by(a.col('name')).having(n > 2).order_by(n.desc(nulls=p.Nulls.Last), a.col('name').asc()).limit(5).offset(1)",
            expected,
        )
    })
}

// [spec:pgorm:req:python.statements/test]
#[test]
fn inserts_and_defaults_match_rust_builders() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let mut expected = Query::insert();
        expected
            .into_table(("application", "account"))
            .columns(["id", "name"]);
        expected
            .values([Expr::value(1i64), Expr::value("Alice")])
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        expected
            .values([SimpleExpr::Constant(2i64.into()), Expr::value("O'Brien")])
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        expected.returning(Query::returning().columns(["id", "name"]));
        parity(
            py,
            &globals,
            "p.Insert(p.Table('account', schema='application')).columns('id', 'name').values(1, 'Alice').values(p.literal(2), \"O'Brien\").returning(p.col('id'), p.col('name'))",
            expected.build(),
        )?;
        parity(
            py,
            &globals,
            "p.Insert(p.Table('account')).default_values().returning()",
            Query::insert()
                .into_table("account")
                .or_default_values()
                .returning_all()
                .build(),
        )
    })
}

// [spec:pgorm:req:python.statements/test]
#[test]
fn conflict_actions_keep_rust_typed_states() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let target = OnConflict::column("id").and_where(Expr::col("id").gt(0i64));
        let action = target
            .update_column("name")
            .value("visits", SimpleExpr::Constant(1i64.into()))
            .and_where(Expr::col("active").eq(true));
        let mut expected = Query::insert();
        expected
            .into_table("account".into_named_table().alias("a"))
            .columns(["id", "name"]);
        expected
            .values([Expr::value(1i64), Expr::value("Alice")])
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        expected.on_conflict(action);
        parity(
            py,
            &globals,
            "p.Insert(p.Table('account', alias='a')).columns('id', 'name').values(1, 'Alice').on_conflict(p.ConflictTarget('id').where_(p.col('id') > 0).update('name').set('visits', p.literal(1)).where_(p.col('active') == True))",
            expected.build(),
        )
    })
}

// [spec:pgorm:req:python.statements/test]
#[test]
fn update_and_delete_keep_builder_parameter_order() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let table = ("application", "account").into_named_table().alias("a");
        parity(
            py,
            &globals,
            "p.Update(p.Table('account', schema='application', alias='a')).set('name', \"new' name\").where_(p.col('id', table='a') == 4).returning(p.col('id'))",
            Query::update()
                .table(table.clone())
                .value("name", "new' name")
                .and_where(Expr::col(("a", "id")).eq(4i64))
                .returning_col("id")
                .build(),
        )?;
        parity(
            py,
            &globals,
            "p.Delete(p.Table('account', schema='application', alias='a')).where_(p.col('id', table='a') == 4).returning()",
            Query::delete()
                .from_table(table)
                .and_where(Expr::col(("a", "id")).eq(4i64))
                .returning_all()
                .build(),
        )
    })
}

// [spec:pgorm:req:python.raw/test]
#[test]
fn raw_templates_use_the_rust_lexer() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let template = "SELECT $2::text, $1::int8, '$9', $$ $8 $$ /* $7 */";
        let values = Values(vec![1i64.into(), "O'Brien".into()]);
        globals.set_item("template", template)?;
        let query = py.eval(
            c"p.RawSQL(template, [1, \"O'Brien\"])",
            Some(&globals),
            None,
        )?;
        let compiled = super::compile(&query)?;
        assert_eq!(compiled.sql, template);
        assert_eq!(compiled.values, values);
        let rendered = query.call_method0("inline_sql")?.extract::<String>()?;
        let expected = pgorm::pgorm_query::inject_parameters(template, values.0)
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        assert_eq!(rendered, expected);
        Ok(())
    })
}
