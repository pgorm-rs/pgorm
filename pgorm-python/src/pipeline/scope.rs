//! A Python callback owns recipes; only its Rust closure owns branded Exprs.

use std::sync::{Arc, Mutex};

use pgorm::{pgorm_query::Value, pipeline as pl};
use pyo3::{
    prelude::*,
    types::{PyList, PyTuple},
};

use super::{expression::PyPipelineExpr, recipe::Recipe};
use crate::{
    UnsupportedCapabilityError,
    errors::{ConstructionError, InternalError, LifecycleError},
    values::PyValue,
};

#[derive(Debug)]
struct State {
    active: bool,
    values: Vec<Value>,
}

#[derive(Debug)]
pub(super) struct Scope(Mutex<State>);

impl Scope {
    pub(super) fn check(&self) -> PyResult<()> {
        if self
            .0
            .lock()
            .map_err(|_| InternalError::new_err("pipeline binder state is poisoned"))?
            .active
        {
            Ok(())
        } else {
            Err(LifecycleError::new_err("pipeline binder scope has ended"))
        }
    }

    fn bind(&self, value: Value) -> PyResult<usize> {
        let mut state = self
            .0
            .lock()
            .map_err(|_| InternalError::new_err("pipeline binder state is poisoned"))?;
        if !state.active {
            return Err(LifecycleError::new_err("pipeline binder scope has ended"));
        }
        let index = state.values.len();
        if index >= 65535 {
            return Err(ConstructionError::new_err(
                "PostgreSQL supports at most 65535 query parameters",
            ));
        }
        state.values.push(value);
        Ok(index)
    }
}

struct CloseScope(Arc<Scope>);

impl Drop for CloseScope {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.0.lock() {
            state.active = false;
            state.values.clear();
        }
    }
}

#[pyclass(name = "PipelineBinder", module = "pgorm.pipeline", frozen)]
#[derive(Debug)]
pub struct PyPipelineBinder {
    scope: Arc<Scope>,
}

#[pymethods]
impl PyPipelineBinder {
    fn bind(&self, value: &Bound<'_, PyAny>) -> PyResult<PyPipelineExpr> {
        self.scope.check()?;
        let value = PyValue::coerce(value)?;
        if value.enum_cast().is_some() {
            return Err(UnsupportedCapabilityError::new_err(
                "pipeline Binder has no qualified enum cast API; bind a plain string when database type inference is intended",
            ));
        }
        let index = self.scope.bind(value.rust_value().clone())?;
        Ok(PyPipelineExpr {
            recipe: Arc::new(Recipe::Bound(index)),
            scope: Some(self.scope.clone()),
        })
    }

    fn __repr__(&self) -> &'static str {
        "PipelineBinder(...)"
    }
}

pub(super) struct BoundPlan {
    pub(super) nodes: Vec<Arc<Recipe>>,
    values: Vec<Value>,
}

impl BoundPlan {
    // [spec:pgorm:req:python.pipeline]
    pub(super) fn lower<'brand, const N: usize>(
        &self,
        binder: &mut pl::Binder<'brand>,
    ) -> [pl::Expr<'brand>; N] {
        let bound: Vec<_> = self
            .values
            .iter()
            .cloned()
            .map(|value| binder.bind(value))
            .collect();
        std::array::from_fn(|index| self.nodes[index].lower(&bound))
    }
}

// [spec:pgorm:req:python.pipeline]
pub(super) fn callback(function: &Bound<'_, PyAny>, single: bool) -> PyResult<BoundPlan> {
    if !function.is_callable() {
        return Err(ConstructionError::new_err(
            "pipeline bound stages require a synchronous callable",
        ));
    }
    let scope = Arc::new(Scope(Mutex::new(State {
        active: true,
        values: Vec::new(),
    })));
    let _close = CloseScope(scope.clone());
    let binder = Py::new(
        function.py(),
        PyPipelineBinder {
            scope: scope.clone(),
        },
    )?;
    let result = function.call1((binder,))?;
    let expressions = if single || result.is_instance_of::<PyPipelineExpr>() {
        vec![PyPipelineExpr::coerce(&result)?]
    } else {
        if !result.is_instance_of::<PyList>() && !result.is_instance_of::<PyTuple>() {
            return Err(ConstructionError::new_err(
                "bound projection callback must return a pipeline expression, list or tuple",
            ));
        }
        result
            .try_iter()?
            .map(|value| PyPipelineExpr::coerce(&value?))
            .collect::<PyResult<Vec<_>>>()?
    };
    for expression in &expressions {
        expression.check()?;
        if expression
            .scope
            .as_ref()
            .is_some_and(|owner| !Arc::ptr_eq(owner, &scope))
        {
            return Err(LifecycleError::new_err(
                "pipeline expression belongs to another binder scope",
            ));
        }
    }
    let mut state = scope
        .0
        .lock()
        .map_err(|_| InternalError::new_err("pipeline binder state is poisoned"))?;
    state.active = false;
    Ok(BoundPlan {
        nodes: expressions
            .into_iter()
            .map(|expression| expression.recipe)
            .collect(),
        values: std::mem::take(&mut state.values),
    })
}
