use std::ffi::CString;

use pgorm::pgorm_query::{
    Asterisk, BinOper, CommonTableExpression, Condition, Expr, Func, IntoNamedTable, JoinType,
    MatchedAction, MergeInsert, MergeUpdate, Name, NotMatchedAction, NullOrdering, OnConflict,
    Order, Overriding, Query, ReturningRow, SimpleExpr, Values, WithClause,
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

// [spec:pgorm:req:python.statements+2/test]
#[test]
fn select_structure_matches_rust_builders() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let a = (Name::runtime("application"), Name::runtime("account"))
            .into_named_table()
            .alias(Name::runtime("a"));
        let e = (Name::runtime("application"), Name::runtime("event"))
            .into_named_table()
            .alias(Name::runtime("e"));
        let count: SimpleExpr =
            Func::count(Expr::col((Name::runtime("e"), Name::runtime("id")))).into();
        let expected = Query::select()
            .column((Name::runtime("a"), Name::runtime("name")))
            .expr_as(count.clone(), Name::runtime("events"))
            .from(a)
            .join(
                JoinType::LeftJoin,
                e,
                Expr::col((Name::runtime("a"), Name::runtime("id")))
                    .equals((Name::runtime("e"), Name::runtime("account_id"))),
            )
            .cond_where(
                Condition::all()
                    .add(Expr::col((Name::runtime("a"), Name::runtime("active"))).eq(true))
                    .add(
                        Condition::any()
                            .add(Expr::col((Name::runtime("e"), Name::runtime("kind"))).eq("click"))
                            .add(Expr::col((Name::runtime("e"), Name::runtime("id"))).is_null()),
                    ),
            )
            .add_group_by([Expr::col((Name::runtime("a"), Name::runtime("name"))).into()])
            .cond_having(Expr::expr(count.clone()).gt(2i64))
            .order_by_expr_with_nulls(count, Order::Desc, NullOrdering::Last)
            .order_by((Name::runtime("a"), Name::runtime("name")), Order::Asc)
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

// [spec:pgorm:req:python.statements+2/test]
#[test]
fn inserts_and_defaults_match_rust_builders() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let mut expected = Query::insert();
        expected
            .into_table((Name::runtime("application"), Name::runtime("account")))
            .columns([Name::runtime("id"), Name::runtime("name")]);
        expected
            .values([Expr::value(1i64), Expr::value("Alice")])
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        expected
            .values([SimpleExpr::Constant(2i64.into()), Expr::value("O'Brien")])
            .map_err(|error| pyo3::exceptions::PyValueError::new_err(error.to_string()))?;
        expected
            .returning(Query::returning().columns([Name::runtime("id"), Name::runtime("name")]));
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
                .into_table(Name::runtime("account"))
                .or_default_values()
                .returning_all()
                .build(),
        )
    })
}

// [spec:pgorm:req:python.statements+2/test]
#[test]
fn conflict_actions_keep_rust_typed_states() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let target = OnConflict::column(Name::runtime("id"))
            .and_where(Expr::col(Name::runtime("id")).gt(0i64));
        let action = target
            .update_column(Name::runtime("name"))
            .value(Name::runtime("visits"), SimpleExpr::Constant(1i64.into()))
            .and_where(Expr::col(Name::runtime("active")).eq(true));
        let mut expected = Query::insert();
        expected
            .into_table(
                Name::runtime("account")
                    .into_named_table()
                    .alias(Name::runtime("a")),
            )
            .columns([Name::runtime("id"), Name::runtime("name")]);
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

// [spec:pgorm:req:python.statements+2/test]
#[test]
fn update_and_delete_keep_builder_parameter_order() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let table = (Name::runtime("application"), Name::runtime("account"))
            .into_named_table()
            .alias(Name::runtime("a"));
        parity(
            py,
            &globals,
            "p.Update(p.Table('account', schema='application', alias='a')).set('name', \"new' name\").where_(p.col('id', table='a') == 4).returning(p.col('id'))",
            Query::update()
                .table(table.clone())
                .value(Name::runtime("name"), "new' name")
                .and_where(Expr::col((Name::runtime("a"), Name::runtime("id"))).eq(4i64))
                .returning_col(Name::runtime("id"))
                .build(),
        )?;
        parity(
            py,
            &globals,
            "p.Delete(p.Table('account', schema='application', alias='a')).where_(p.col('id', table='a') == 4).returning()",
            Query::delete()
                .from_table(table)
                .and_where(Expr::col((Name::runtime("a"), Name::runtime("id"))).eq(4i64))
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

// [spec:pgorm:req:python.statements+2/test]
#[test]
fn merge_arms_match_the_rust_typestate() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let t = |column: &str| Expr::col((Name::runtime("t"), Name::runtime(column)));
        let s = |column: &str| Expr::col((Name::runtime("s"), Name::runtime(column)));
        let mut expected = Query::merge(
            (Name::runtime("application"), Name::runtime("account"))
                .into_named_table()
                .alias(Name::runtime("t")),
            Name::runtime("staged")
                .into_named_table()
                .alias(Name::runtime("s")),
            t("id").equals((Name::runtime("s"), Name::runtime("id"))),
        )
        .when_matched(MergeUpdate::value(Name::runtime("name"), s("name")))
        .to_owned();
        expected
            .when_matched_and(s("visits").gt(10i64), MatchedAction::Delete)
            .when_matched_and(s("visits").gt(1i64), MatchedAction::DoNothing)
            .when_not_matched_and(
                s("name").eq("O'Brien"),
                MergeInsert::value(Name::runtime("id"), s("id"))
                    .and_value(Name::runtime("name"), SimpleExpr::Constant("x".into()))
                    .overriding(Overriding::UserValue),
            )
            .when_not_matched(NotMatchedAction::InsertDefaultValues)
            .when_not_matched_by_source_and(t("visits").eq(0i64), MatchedAction::Delete)
            .when_not_matched_by_source(
                MergeUpdate::value(Name::runtime("visits"), 0i64)
                    .and_value(Name::runtime("name"), "gone"),
            )
            .returning_action()
            .returning(
                Query::returning()
                    .exprs::<SimpleExpr, _>([
                        Expr::col((ReturningRow::Old, Name::runtime("visits"))).into(),
                        Expr::col((ReturningRow::New, Asterisk)).into(),
                    ])
                    .old_as(Name::runtime("before")),
            )
            .only();
        py.run(
            c"t = p.Table('account', schema='application', alias='t')\ns = p.Table('staged', alias='s')",
            Some(&globals),
            None,
        )?;
        parity(
            py,
            &globals,
            "p.merge(t, s, t.col('id') == s.col('id'))\
             .when_matched(p.MergeUpdate('name', s.col('name')))\
             .when_matched(p.MatchedAction.Delete, condition=s.col('visits') > 10)\
             .when_matched(p.MatchedAction.DoNothing, condition=s.col('visits') > 1)\
             .when_not_matched(p.MergeInsert('id', s.col('id')).and_value('name', p.literal('x'))\
                 .overriding(p.Overriding.UserValue), condition=s.col('name') == \"O'Brien\")\
             .when_not_matched(p.NotMatchedAction.InsertDefaultValues)\
             .when_not_matched_by_source(p.MatchedAction.Delete, condition=t.col('visits') == 0)\
             .when_not_matched_by_source(p.MergeUpdate('visits', 0).and_value('name', 'gone'))\
             .returning_action()\
             .returning(p.ReturningRow.Old.col('visits'), p.ReturningRow.New.star(), old_as='before')\
             .only()",
            expected.build(),
        )
    })
}

// [spec:pgorm:req:python.statements+2/test]
#[test]
fn merge_reads_and_feeds_common_table_expressions() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let staged = Query::select()
            .expr(Expr::col(Name::runtime("id")))
            .from(Name::runtime("source"))
            .to_owned();
        let merge = Query::merge(
            Name::runtime("account"),
            Name::runtime("staged"),
            Expr::col((Name::runtime("account"), Name::runtime("id")))
                .equals((Name::runtime("staged"), Name::runtime("id"))),
        )
        .when_matched(MatchedAction::Delete)
        .with(WithClause::new(CommonTableExpression::new(
            Name::runtime("staged"),
            staged,
        )))
        .returning(Query::returning().all())
        .to_owned();
        let expected = Query::select()
            .expr(Expr::col(Asterisk))
            .from(Name::runtime("m"))
            .with(WithClause::new(CommonTableExpression::new(
                Name::runtime("m"),
                merge,
            )))
            .build();
        py.run(
            c"a = p.Table('account')\nstaged = p.Table('staged')\nm = p.merge(a, staged, a.col('id') == staged.col('id')).when_matched(p.MatchedAction.Delete).with_(p.With('staged', p.Select(p.col('id')).from_(p.Table('source')))).returning()",
            Some(&globals),
            None,
        )?;
        parity(
            py,
            &globals,
            "p.Select().from_(p.Table('m')).with_(p.With('m', m))",
            expected,
        )
    })
}

// [spec:pgorm:req:python.statements+2/test]
#[test]
fn returning_reads_and_renames_row_versions() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        parity(
            py,
            &globals,
            "p.Update(p.Table('account')).set('visits', 1).where_(p.col('id') == 4)\
             .returning(p.ReturningRow.Old.col('visits'), p.col('visits', table='n').as_('after'), new_as='n')",
            Query::update()
                .table(Name::runtime("account"))
                .value(Name::runtime("visits"), 1i64)
                .and_where(Expr::col(Name::runtime("id")).eq(4i64))
                .returning(
                    Query::returning()
                        .exprs([
                            Expr::col((ReturningRow::Old, Name::runtime("visits"))).into(),
                            Expr::col((Name::runtime("n"), Name::runtime("visits")))
                                .binary(BinOper::As, Expr::col(Name::runtime("after"))),
                        ])
                        .new_as(Name::runtime("n")),
                )
                .build(),
        )?;
        parity(
            py,
            &globals,
            "p.Delete(p.Table('account')).all_rows().returning(old_as='o', new_as='n')",
            Query::delete()
                .from_table(Name::runtime("account"))
                .returning(
                    Query::returning()
                        .all()
                        .old_as(Name::runtime("o"))
                        .new_as(Name::runtime("n")),
                )
                .build(),
        )?;
        parity(
            py,
            &globals,
            "p.Insert(p.Table('account')).columns('id').values(1).returning(p.ReturningRow.Old.star())",
            Query::insert()
                .into_table(Name::runtime("account"))
                .columns([Name::runtime("id")])
                .values_panic([Expr::value(1i64)])
                .returning(Query::returning().expr(Expr::col((ReturningRow::Old, Asterisk))))
                .build(),
        )
    })
}

// [spec:pgorm:req:python.statements+2/test]
#[test]
fn merge_actions_are_typed_by_their_row() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        py.run(
            c"a = p.Table('a')\nb = p.Table('b')\npending = p.merge(a, b, a.col('id') == b.col('id'))",
            Some(&globals),
            None,
        )?;
        for source in [
            "pending.when_matched(p.MergeInsert('id', 1))",
            "pending.when_matched(p.NotMatchedAction.DoNothing)",
            "pending.when_not_matched_by_source(p.MergeInsert('id', 1))",
            "pending.when_not_matched(p.MergeUpdate('id', 1))",
            "pending.when_not_matched(p.MatchedAction.Delete)",
            "pending.when_matched(p.MatchedAction.Delete).when_not_matched(p.MatchedAction.Delete)",
            "p.With('w', p.Insert(a).default_values())",
        ] {
            let error = py
                .eval(&CString::new(source)?, Some(&globals), None)
                .expect_err(source);
            assert!(
                error.is_instance_of::<pyo3::exceptions::PyTypeError>(py),
                "{source}: {error}"
            );
        }
        let pending = py.eval(c"pending", Some(&globals), None)?;
        let error = super::compile(&pending).expect_err("a pending MERGE has no statement");
        assert!(error.to_string().contains("needs a WHEN arm"));
        assert!(!pending.hasattr("inspect")?);
        Ok(())
    })
}
