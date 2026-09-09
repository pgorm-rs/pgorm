use std::ffi::CString;

use pgorm::pgorm_query::{Condition, Expr, Func, LikeExpr, Query, SimpleExpr, TypeName, Value};
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
// [spec:pgorm:req:python.expressions/test]
#[test]
fn expression_structure_matches_rust_builders() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let column: SimpleExpr = Expr::col(("schema", "table", "x")).into();
        let cases = [
            ("p.col('x', table='table', schema='schema')", column.clone()),
            (
                "p.bind(p.Value(7, 'i16')) + p.literal(3)",
                Expr::value(7i16).add(SimpleExpr::Constant(3i64.into())),
            ),
            (
                "(p.col('a') == 1) & ((p.col('b') > 2) | p.col('c').is_null())",
                Expr::col("a")
                    .eq(1i64)
                    .and(Expr::col("b").gt(2i64).or(Expr::col("c").is_null())),
            ),
            ("~(p.col('x') < 3)", Expr::col("x").lt(3i64).not()),
            (
                "p.col('x').between(p.literal(1), 2)",
                Expr::col("x").between(SimpleExpr::Constant(1i64.into()), Expr::value(2i64)),
            ),
            (
                "p.col('x').not_between(1, 2)",
                Expr::col("x").not_between(1i64, 2i64),
            ),
            (
                "p.col('x').is_in([1, p.literal(2)])",
                Expr::col("x").is_in([Expr::value(1i64), SimpleExpr::Constant(2i64.into())]),
            ),
            (
                "p.col('x').is_in([])",
                Expr::col("x").is_in(Vec::<SimpleExpr>::new()),
            ),
            (
                "p.col('x').is_not_in([])",
                Expr::col("x").is_not_in(Vec::<SimpleExpr>::new()),
            ),
            (
                "p.tuple_expr(p.col('x'), 2)",
                Expr::tuple([Expr::col("x").into(), Expr::value(2i64)]).into(),
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
            Expr::value("calm").cast_as_type(TypeName::new("Mood").schema("Tenant")),
        )?;
        parity(
            py,
            &globals,
            "p.literal(p.Value('calm', p.TypeName('Mood', schema='Tenant')))",
            SimpleExpr::Constant(Value::from("calm"))
                .cast_as_type(TypeName::new("Mood").schema("Tenant")),
        )?;
        parity(
            py,
            &globals,
            "p.col('x').cast(p.TypeName('integer'), array=True)",
            Expr::col("x").cast_as_type(TypeName::new("integer").array()),
        )
    })
}

// [spec:pgorm:req:python.expressions/test]
#[test]
fn substring_and_pattern_paths_use_rust_functions() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let text: SimpleExpr = Expr::col("text").into();
        parity(
            py,
            &globals,
            "p.col('text').starts_with('%_')",
            Func::starts_with(text.clone(), "%_").into(),
        )?;
        let position = Func::cust("strpos").args([text.clone(), Expr::value("%_")]);
        parity(
            py,
            &globals,
            "p.col('text').contains_text('%_')",
            Expr::expr(position).gt(SimpleExpr::Constant(0i32.into())),
        )?;
        let suffix = Func::cust("right").args([text, Func::char_length("%_").into()]);
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
            Expr::col("text").like(LikeExpr::new("!%_").escape('!')),
        )?;
        parity(
            py,
            &globals,
            "p.col('text').ilike(p.LikePattern('!%_', escape='!'))",
            Expr::col("text").ilike(LikeExpr::new("!%_").escape('!')),
        )
    })
}

// [spec:pgorm:req:python.expressions/test]
#[test]
fn function_calls_use_named_rust_constructors() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let globals = module(py)?;
        let x = Expr::col("x");
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
            ("coalesce", Func::coalesce([x.into()])),
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
        )
    })
}

// [spec:pgorm:req:python.expressions/test]
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
                Condition::all().add(Expr::col("a").eq(1i64)).add(
                    Condition::any()
                        .add(Expr::col("b").eq(2i64))
                        .add(Expr::col("c").eq(3i64)),
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
