use pgorm::pgorm_query::{NullOrdering, Order};
use pgorm::{
    ActiveModelBehavior, ActiveModelTrait, ActiveValue, ColumnTrait, EntityTrait, IdenStr,
    Iterable, QueryFilter, QueryOrder, QuerySelect, QueryTrait,
};
use pgorm_python::{entities::ActiveState, expressions::Compiled, values::PyValue};
use pyo3::{prelude::*, types::PyDict};

use crate::account::{self, Column};

fn module(py: Python<'_>) -> PyResult<Bound<'_, PyModule>> {
    let module = PyModule::new(py, "pgorm._native")?;
    crate::_native(&module)?;
    Ok(module)
}

// [spec:pgorm:req:python.entities/test]
#[test]
fn query_sql_and_values_match_rust() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let module = module(py)?;
        let entity = module.call_method1("entity", ("app.Account",))?;
        let id = entity.call_method1("col", ("id",))?;
        let mood = entity.call_method1("col", ("mood",))?;
        let descending = id.call_method0("expr")?.call_method0("desc")?;
        let comparisons = [
            ("eq", Column::Id.eq(2)),
            ("ne", Column::Id.ne(2)),
            ("gt", Column::Id.gt(2)),
            ("gte", Column::Id.gte(2)),
            ("lt", Column::Id.lt(2)),
            ("lte", Column::Id.lte(2)),
        ];
        for (method, predicate) in comparisons {
            let query = entity
                .call_method0("find")?
                .call_method1("filter", (id.call_method1(method, (2,))?,))?
                .call_method1("filter", (mood.call_method1("eq", ("busy",))?,))?
                .call_method1("order_by", (&descending,))?
                .call_method1("limit", (4,))?
                .call_method1("offset", (1,))?;
            let rust = account::Entity::find()
                .filter(predicate)
                .filter(Column::Mood.eq("busy"))
                .order_by(Column::Id, Order::Desc)
                .limit(4)
                .offset(1);
            for terminal in ["all", "one", "one_opt"] {
                let kwargs = PyDict::new(py);
                kwargs.set_item("terminal", terminal)?;
                let inspected = query.call_method("inspect", (), Some(&kwargs))?;
                let native = inspected.extract::<PyRef<'_, Compiled>>()?;
                let expected = if terminal == "all" {
                    rust.clone()
                } else {
                    rust.clone().limit(1)
                }
                .build();
                assert_eq!(native.sql, expected.0, "{method}/{terminal}");
                assert_eq!(native.values, expected.1, "{method}/{terminal}");
            }
        }
        let kwargs = PyDict::new(py);
        kwargs.set_item("nulls", module.getattr("Nulls")?.getattr("First")?)?;
        let order = id
            .call_method0("expr")?
            .call_method("asc", (), Some(&kwargs))?;
        let native: Compiled = entity
            .call_method0("find")?
            .call_method1("order_by", (order,))?
            .call_method1("limit", (1,))?
            .call_method0("limit")?
            .call_method1("offset", (1,))?
            .call_method0("offset")?
            .call_method0("inspect")?
            .extract()?;
        let expected = account::Entity::find()
            .order_by_with_nulls(Column::Id, Order::Asc, NullOrdering::First)
            .build();
        assert_eq!((native.sql, native.values), expected);
        Ok(())
    })
}

fn assert_active(native: &Bound<'_, PyAny>, rust: &account::ActiveModel) -> PyResult<()> {
    for column in Column::iter() {
        let native = native.call_method1("get", (column.as_str(),))?;
        let state = native.getattr("state")?.extract::<ActiveState>()?;
        let value = native.getattr("value")?;
        let expected = match rust.get(column) {
            ActiveValue::NotSet => {
                assert_eq!(state, ActiveState::NotSet);
                assert!(value.is_none());
                continue;
            }
            ActiveValue::Set(value) => {
                assert_eq!(state, ActiveState::Set);
                value
            }
            ActiveValue::Unchanged(value) => {
                assert_eq!(state, ActiveState::Unchanged);
                value
            }
        };
        assert_eq!(
            value.extract::<PyRef<'_, PyValue>>()?.rust_value(),
            &expected
        );
    }
    Ok(())
}

// [spec:pgorm:req:python.entities/test]
#[test]
fn active_state_and_values_match_rust() -> PyResult<()> {
    Python::initialize();
    Python::attach(|py| {
        let module = module(py)?;
        let entity = module.call_method1("entity", ("app.Account",))?;
        let initial = entity.call_method0("active")?;
        let mut rust = account::ActiveModel::new();
        assert_active(&initial, &rust)?;
        let active = initial
            .call_method1("set", ("id", 41))?
            .call_method1("set", ("display name", "O'Brien"))?
            .call_method1("set", ("note", py.None()))?
            .call_method1("set", ("mood", "busy"))?;
        rust.set(Column::Id, 41.into()).unwrap();
        rust.set(Column::Name, "O'Brien".into()).unwrap();
        rust.set(Column::Note, None::<String>.into()).unwrap();
        rust.set(Column::Mood, "busy".into()).unwrap();
        assert_active(&active, &rust)?;
        assert_active(&initial, &account::ActiveModel::new())?;
        let active = active
            .call_method1("not_set", ("note",))?
            .call_method1("reset", ("id",))?;
        rust.not_set(Column::Note);
        rust.reset(Column::Id);
        assert_active(&active, &rust)
    })
}
