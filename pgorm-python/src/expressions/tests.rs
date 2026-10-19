use std::ffi::CString;

use pgorm::pgorm_query::{
    Condition, Expr, Func, LikeExpr, Name, Query, SimpleExpr, TypeName, Value,
};
use pyo3::{prelude::*, types::PyDict};

use super::{Compiled, PyExpr};

fn module(py: Python<'_>) -> PyResult<Bound<'_, PyDict>> {
    let module = PyModule::new(py, "pgorm")?;
    crate::values::register(&module)?;
    crate::identifiers::register(&module)?;
    super::register(&module)?;
    let globals = PyDict::new(py);
    globals.set_item("p", module)?;
    Ok(globals)
}

fn parity(
    py: Python<'_>,
    globals: &Bound<'_, PyDict>,
    source: &str,
    expected: SimpleExpr,
) -> PyResult<()> {
    let source = CString::new(source)?;
    let actual = py.eval(&source, Some(globals), None)?;
    assert_eq!(actual.extract::<PyRef<'_, PyExpr>>()?.inner, expected);
    let (sql, values) = Query::select().expr(expected).build();
    let built = actual.call_method0("inspect")?;
    let built = built.extract::<PyRef<'_, Compiled>>()?;
    assert_eq!(built.sql, sql);
    assert_eq!(built.values, values);
    Ok(())
}

// [spec:pgorm:req:python.delegation+1/test]
// [spec:pgorm:req:python.expressions+2/test]
#[test]
fn expression_structure_matches_rust_builders() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let column: SimpleExpr = Expr::col((
            Name::runtime("schema"),
            Name::runtime("table"),
            Name::runtime("x"),
        ))
        .into();
        let cases = [
            ("p.col('x', table='table', schema='schema')", column.clone()),
            (
                "p.bind(p.Value(7, 'i16')) + p.literal(3)",
                Expr::value(7i16).add(SimpleExpr::Constant(3i64.into())),
            ),
            (
                "(p.col('a') == 1) & ((p.col('b') > 2) | p.col('c').is_null())",
                Expr::col(Name::runtime("a")).eq(1i64).and(
                    Expr::col(Name::runtime("b"))
                        .gt(2i64)
                        .or(Expr::col(Name::runtime("c")).is_null()),
                ),
            ),
            (
                "~(p.col('x') < 3)",
                Expr::col(Name::runtime("x")).lt(3i64).not(),
            ),
            (
                "p.col('x').between(p.literal(1), 2)",
                Expr::col(Name::runtime("x"))
                    .between(SimpleExpr::Constant(1i64.into()), Expr::value(2i64)),
            ),
            (
                "p.col('x').not_between(1, 2)",
                Expr::col(Name::runtime("x")).not_between(1i64, 2i64),
            ),
            (
                "p.col('x').is_in([1, p.literal(2)])",
                Expr::col(Name::runtime("x"))
                    .is_in([Expr::value(1i64), SimpleExpr::Constant(2i64.into())]),
            ),
            (
                "p.col('x').is_in([])",
                Expr::col(Name::runtime("x")).is_in(Vec::<SimpleExpr>::new()),
            ),
            (
                "p.col('x').is_not_in([])",
                Expr::col(Name::runtime("x")).is_not_in(Vec::<SimpleExpr>::new()),
            ),
            (
                "p.tuple_expr(p.col('x'), 2)",
                Expr::tuple([Expr::col(Name::runtime("x")).into(), Expr::value(2i64)]).into(),
            ),
        ];
        for (source, expected) in cases {
            parity(py, &globals, source, expected)?;
        }
        Ok(())
    })
}

// [spec:pgorm:req:python.input-boundaries/test]
#[test]
fn qualified_casts_preserve_source_typed_parameters() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        parity(
            py,
            &globals,
            "p.bind(p.Value('calm', p.TypeName('Mood', schema='Tenant')))",
            Expr::value("calm")
                .cast_as_type(TypeName::new(Name::runtime("Mood")).schema(Name::runtime("Tenant"))),
        )?;
        parity(
            py,
            &globals,
            "p.literal(p.Value('calm', p.TypeName('Mood', schema='Tenant')))",
            SimpleExpr::Constant(Value::from("calm"))
                .cast_as_type(TypeName::new(Name::runtime("Mood")).schema(Name::runtime("Tenant"))),
        )?;
        parity(
            py,
            &globals,
            "p.col('x').cast(p.TypeName('integer'), array=True)",
            Expr::col(Name::runtime("x"))
                .cast_as_type(TypeName::new(Name::runtime("integer")).array()),
        )
    })
}

// [spec:pgorm:req:python.expressions+2/test]
#[test]
fn substring_and_pattern_paths_use_rust_functions() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let text: SimpleExpr = Expr::col(Name::runtime("text")).into();
        parity(
            py,
            &globals,
            "p.col('text').starts_with('%_')",
            Func::starts_with(text.clone(), "%_").into(),
        )?;
        let position = Func::named(Name::runtime("strpos")).args([text.clone(), Expr::value("%_")]);
        parity(
            py,
            &globals,
            "p.col('text').contains_text('%_')",
            Expr::expr(position).gt(SimpleExpr::Constant(0i32.into())),
        )?;
        let suffix =
            Func::named(Name::runtime("right")).args([text, Func::char_length("%_").into()]);
        parity(
            py,
            &globals,
            "p.col('text').ends_with('%_')",
            Expr::expr(suffix).eq("%_"),
        )?;
        parity(
            py,
            &globals,
            "p.col('text').like(p.LikePattern('!%_', escape='!'))",
            Expr::col(Name::runtime("text")).like(LikeExpr::new("!%_").escape('!')),
        )?;
        parity(
            py,
            &globals,
            "p.col('text').ilike(p.LikePattern('!%_', escape='!'))",
            Expr::col(Name::runtime("text")).ilike(LikeExpr::new("!%_").escape('!')),
        )?;
        parity(
            py,
            &globals,
            "p.col('text').not_like(p.LikePattern('a\\\\%', escape=''))",
            Expr::col(Name::runtime("text")).not_like(LikeExpr::new("a\\%").no_escape()),
        )
    })
}

// [spec:pgorm:req:python.expressions+2/test]
#[test]
fn function_calls_use_named_rust_constructors() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let x = Expr::col(Name::runtime("x"));
        let cases = [
            ("lower", Func::lower(x.clone())),
            ("upper", Func::upper(x.clone())),
            ("abs", Func::abs(x.clone())),
            ("char_length", Func::char_length(x.clone())),
            ("count", Func::count(x.clone())),
            ("count_distinct", Func::count_distinct(x.clone())),
            ("sum", Func::sum(x.clone())),
            ("avg", Func::avg(x.clone())),
            ("min", Func::min(x.clone())),
            ("max", Func::max(x.clone())),
            ("round", Func::round(x.clone())),
            ("coalesce", Func::coalesce([x.clone().into()])),
        ];
        for (name, expected) in cases {
            parity(
                py,
                &globals,
                &format!("p.call('{name}', p.col('x'))"),
                expected.into(),
            )?;
        }
        parity(py, &globals, "p.call('random')", Func::random().into())?;
        parity(
            py,
            &globals,
            "p.call('gen_random_uuid')",
            Func::gen_random_uuid().into(),
        )?;
        parity(py, &globals, "p.call('uuidv4')", Func::uuidv4().into())?;
        parity(py, &globals, "p.call('uuidv7')", Func::uuidv7().into())?;
        parity(
            py,
            &globals,
            "p.call('uuidv7', p.col('x'))",
            Func::uuidv7_shifted(x.clone()).into(),
        )?;
        parity(
            py,
            &globals,
            "p.call('uuid_extract_timestamp', p.col('x'))",
            Func::uuid_extract_timestamp(x.clone()).into(),
        )?;
        parity(
            py,
            &globals,
            "p.call('uuid_extract_version', p.col('x'))",
            Func::uuid_extract_version(x.clone()).into(),
        )
    })
}

// [spec:pgorm:req:python.expressions+2/test]
// [spec:pgorm:req:python.ownership/test]
#[test]
fn conditions_keep_rust_empty_and_grouping_semantics() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let cases = [
            ("p.Condition.all()", Condition::all()),
            ("p.Condition.any()", Condition::any()),
            ("~p.Condition.all()", Condition::all().not()),
            (
                "p.Condition.all(p.col('a') == 1, p.Condition.any(p.col('b') == 2, p.col('c') == 3))",
                Condition::all()
                    .add(Expr::col(Name::runtime("a")).eq(1i64))
                    .add(
                        Condition::any()
                            .add(Expr::col(Name::runtime("b")).eq(2i64))
                            .add(Expr::col(Name::runtime("c")).eq(3i64)),
                    ),
            ),
        ];
        for (source, condition) in cases {
            let actual = py
                .eval(&CString::new(source)?, Some(&globals), None)?
                .call_method0("inspect")?;
            let actual = actual.extract::<PyRef<'_, Compiled>>()?;
            let (sql, values) = Query::select()
                .expr(SimpleExpr::Constant(true.into()))
                .cond_where(condition)
                .build();
            assert_eq!(actual.sql, sql);
            assert_eq!(actual.values, values);
        }
        Ok(())
    })
}

// [spec:pgorm:req:python.expressions+2/test]
// [spec:pgorm:req:sql.ast.expr.similar-to+1/test]    both forms through LikePattern, its
// ESCAPE carried, and a plain string refused as LIKE's is
#[test]
fn similar_to_takes_like_patterns() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let text = || Expr::col(Name::runtime("text"));
        parity(
            py,
            &globals,
            "p.col('text').similar_to(p.LikePattern('(a|b)%'))",
            text().similar_to("(a|b)%"),
        )?;
        parity(
            py,
            &globals,
            "p.col('text').not_similar_to(p.LikePattern('!%%', escape='!'))",
            text().not_similar_to(LikeExpr::new("!%%").escape('!')),
        )?;
        let error = py
            .eval(c"p.col('text').similar_to('a%')", Some(&globals), None)
            .expect_err("a plain string is not a pattern");
        assert!(error.is_instance_of::<crate::errors::ConstructionError>(py));
        Ok(())
    })
}

// [spec:pgorm:req:python.expressions+2/test]
// [spec:pgorm:req:sql.ast.select.table-function+2/test]    `call` reaches the set-returning
// constructors at their arities, and no other
#[test]
fn set_returning_calls_use_rust_constructors() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        parity(
            py,
            &globals,
            "p.call('generate_series', p.literal(1), p.col('x'))",
            Func::generate_series(
                SimpleExpr::Constant(1i64.into()),
                Expr::col(Name::runtime("x")),
            )
            .into(),
        )?;
        parity(
            py,
            &globals,
            "p.call('generate_series', 10, 1, -3)",
            Func::generate_series_step(Expr::value(10i64), Expr::value(1i64), Expr::value(-3i64))
                .into(),
        )?;
        parity(
            py,
            &globals,
            "p.call('unnest', p.col('tags'))",
            Func::unnest(Expr::col(Name::runtime("tags"))).into(),
        )?;
        for source in [
            "p.call('generate_series', 1)",
            "p.call('generate_series', 1, 2, 3, 4)",
            "p.call('unnest')",
            "p.call('unnest', p.col('a'), p.col('b'))",
        ] {
            let error = py
                .eval(&CString::new(source)?, Some(&globals), None)
                .expect_err(source);
            assert!(
                error.is_instance_of::<crate::UnsupportedCapabilityError>(py),
                "{source}: {error}"
            );
        }
        Ok(())
    })
}

// [spec:pgorm:req:python.expr-collate/test]    bare and schema-qualified, compared, and as
// an ORDER BY key; a name that is not an identifier is refused
#[test]
fn collations_lower_into_rust_collate() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let name = || Expr::col(Name::runtime("name"));
        parity(
            py,
            &globals,
            "p.col('name').collate('C')",
            name().collate(Name::runtime("C")).into(),
        )?;
        parity(
            py,
            &globals,
            "p.col('name').collate(p.Identifier('C \"x\"'), schema='pg_catalog') < 'b'",
            name()
                .collate((Name::runtime("pg_catalog"), Name::runtime("C \"x\"")))
                .lt("b"),
        )?;
        let source = "p.col('name').collate('C').desc().expr";
        let ordered = py.eval(&CString::new(source)?, Some(&globals), None)?;
        assert_eq!(
            ordered.extract::<PyRef<'_, PyExpr>>()?.inner,
            name().collate(Name::runtime("C")).into()
        );
        for source in [
            "p.col('name').collate('')",
            "p.col('name').collate('C', schema='')",
            "p.col('name').collate(1)",
            "p.col('name').collate('雪' * 22)",
        ] {
            let error = py
                .eval(&CString::new(source)?, Some(&globals), None)
                .expect_err(source);
            assert!(
                error.is_instance_of::<crate::errors::ConstructionError>(py),
                "{source}: {error}"
            );
        }
        Ok(())
    })
}
