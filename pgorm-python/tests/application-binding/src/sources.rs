use crate::{account::Entity as A, note::Entity as N};
use pgorm_python::entities::Registry;
use pyo3::prelude::*;

// [spec:pgorm:req:python.pipeline/test]
pub fn register(registry: &mut Registry) -> PyResult<()> {
    registry.sources::<(A,)>("app.SingleAccount")?;
    registry.sources::<(A, N)>("app.TwoSources")?;
    registry.sources::<(A, N, N)>("app.ThreeSources")?;
    registry.sources::<(A, N, N, N)>("app.FourSources")?;
    registry.sources::<(A, N, N, N, N)>("app.FiveSources")?;
    registry.sources::<(A, N, N, N, N, N)>("app.SixSources")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use pgorm::{
        pgorm_query::Alias,
        pipeline::{self as pl, ExprOps, IntoSource},
    };
    use pgorm_python::expressions::Compiled;
    use pyo3::types::PyDict;

    // [spec:pgorm:req:python.pipeline/test]
    #[test]
    fn source_registration_requires_real_entities() -> PyResult<()> {
        Python::initialize();
        let mut registry = Registry::default();
        assert!(registry.sources::<(A,)>("missing").is_err());
        registry.entity::<A>("app.Account")?;
        assert!(registry.sources::<(A, N)>("missing").is_err());
        registry.entity::<N>("app.Note")?;
        registry.sources::<(A, N)>("valid")?;
        assert!(registry.sources::<(A, N)>("valid").is_err());
        assert!(registry.sources::<(A,)>("").is_err());
        Ok(())
    }

    // [spec:pgorm:req:python.pipeline/test]
    #[test]
    fn source_selection_matches_native_rust_projection() -> PyResult<()> {
        Python::initialize();
        Python::attach(|py| {
            let module = PyModule::new(py, "pgorm._native")?;
            crate::_native(&module)?;
            let account = module.call_method1("entity", ("app.Account",))?;
            let pipeline = module.getattr("Pipeline")?.call1((account,))?;
            let selection = module.call_method1("pipeline_sources", ("app.SingleAccount",))?;
            let selected = pipeline.call_method1("select_sources", (selection,))?;
            for terminal in ["all", "one", "one_opt"] {
                let kwargs = PyDict::new(py);
                kwargs.set_item("terminal", terminal)?;
                let actual: Compiled = selected
                    .call_method("inspect", (), Some(&kwargs))?
                    .extract()?;
                let mut expected = pl::Pipeline::from(A);
                if terminal != "all" {
                    expected = expected.take(1);
                }
                let expected = expected.select_sources(A).into_sql().map_err(|error| {
                    pyo3::exceptions::PyRuntimeError::new_err(error.to_string())
                })?;
                assert_eq!((actual.sql, actual.values), expected);
            }
            let alias = Alias::new("runtime alias \"雪\"");
            let expr: pl::Expr<'_> = alias.clone().into();
            let (sql, _) = pl::Pipeline::from(pl::named_runtime(A, Alias::new("a")))
                .derive(pl::col(Alias::new("a"), Alias::new("id")).as_runtime(alias))
                .filter(expr.gt(1i64))
                .into_sql()
                .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))?;
            let token = pl::alias("runtime alias \"雪\"");
            let (expected, _) = pl::Pipeline::from(A.named("a"))
                .derive(pl::col(Alias::new("a"), Alias::new("id")).as_(token))
                .filter(token.gt(1i64))
                .into_sql()
                .map_err(|error| pyo3::exceptions::PyRuntimeError::new_err(error.to_string()))?;
            assert_eq!(sql, expected);
            Ok(())
        })
    }
}
