use crate::{account, graphs, note};
use pgorm::pgorm_query::{Alias, Expr, Order};
use pgorm::{QueryFilter, QueryOrder, QueryTrait};
use pgorm_python::expressions::Compiled;
use pyo3::{prelude::*, types::PyDict};

// [spec:pgorm:req:python.graph/test]
#[test]
fn graph_sql_and_parameters_match_rust() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let module = PyModule::new(py, "pgorm._native")?;
        crate::_native(&module)?;
        for alias in ["notes", "notes \"β\""] {
            for name in ["app.AccountNotes", "app.RequiredNotes"] {
                let graph = module.call_method1("graph", (name,))?;
                let kwargs = PyDict::new(py);
                kwargs.set_item("aliases", vec![alias])?;
                let query = graph.call_method("find", (), Some(&kwargs))?;
                let id = query.call_method1("col", (0, "id"))?;
                let child = query.call_method1("col", (1, "id"))?;
                let query = query
                    .call_method1("filter", (id.call_method1("gte", (2,))?,))?
                    .call_method1("order_by", (child.call_method0("desc")?,))?;
                for terminal in ["all", "one_opt"] {
                    let kwargs = PyDict::new(py);
                    kwargs.set_item("terminal", terminal)?;
                    let native: Compiled =
                        query.call_method("inspect", (), Some(&kwargs))?.extract()?;
                    let filter = Expr::col((Alias::new("accounts"), account::Column::Id)).gte(2i64);
                    let ordering = Expr::col((Alias::new(alias), note::Column::Id));
                    let mut rust = if name == "app.AccountNotes" {
                        graphs::optional(&[alias.to_owned()])
                            .filter(filter)
                            .order_by(ordering, Order::Desc)
                            .into_query()
                    } else {
                        graphs::required(&[alias.to_owned()])
                            .filter(filter)
                            .order_by(ordering, Order::Desc)
                            .into_query()
                    };
                    if terminal == "one_opt" {
                        rust.limit(1);
                    }
                    assert_eq!(
                        (native.sql, native.values),
                        rust.build(),
                        "{name}/{alias}/{terminal}"
                    );
                }
            }
        }
        Ok(())
    })
}

// [spec:pgorm:req:python.graph/test]
#[test]
fn graphs_require_sources_and_unique_names() -> PyResult<()> {
    Python::initialize();
    let mut registry = pgorm_python::entities::Registry::default();
    assert!(registry.graph("missing", graphs::optional).is_err());
    registry.entity::<account::Entity>("app.Account")?;
    assert!(registry.graph("missing", graphs::optional).is_err());
    registry.entity::<note::Entity>("app.Note")?;
    registry.graph("notes", graphs::optional)?;
    assert!(registry.graph("notes", graphs::required).is_err());
    assert!(registry.graph("", graphs::optional).is_err());
    Ok(())
}
