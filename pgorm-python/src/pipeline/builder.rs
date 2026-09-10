use pgorm::pipeline as pl;
use pyo3::{prelude::*, types::PyTuple};

use super::{
    bound::{self, Stage},
    construct::integer,
    expression::PyPipelineExpr,
    scope::callback,
    source::pipeline_source,
    window::{PyOver, expressions},
};
use crate::{
    entities, errors::ConstructionError, expressions::Compiled, results, statements::Join,
};

// [spec:pgorm:req:python.pipeline]
/// An immutable wrapper owning the real Rust Pipeline, including its bound values.
#[pyclass(name = "Pipeline", module = "pgorm.pipeline", frozen, from_py_object)]
#[derive(Clone, Debug)]
pub struct PyPipeline {
    pub inner: pl::Pipeline,
}

pub(super) fn join_side(kind: Join) -> pl::JoinSide {
    match kind {
        Join::Inner => pl::JoinSide::Inner,
        Join::Left => pl::JoinSide::Left,
        Join::Right => pl::JoinSide::Right,
        Join::Full => pl::JoinSide::Full,
    }
}

impl PyPipeline {
    pub(crate) fn compile(&self, terminal: &str) -> PyResult<Compiled> {
        let pipeline = match terminal {
            "all" => self.inner.clone(),
            "one" | "one_opt" => self.inner.clone().take(1),
            _ => return Err(ConstructionError::new_err("unknown pipeline terminal")),
        };
        let (sql, values) = pipeline
            .into_sql()
            .map_err(|error| ConstructionError::new_err(error.to_string()))?;
        if values.0.len() > 65535 {
            return Err(ConstructionError::new_err(
                "PostgreSQL supports at most 65535 query parameters",
            ));
        }
        Ok(Compiled { sql, values })
    }

    fn read<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
        terminal: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let compiled = Py::new(py, self.compile(terminal)?)?
            .into_bound(py)
            .into_any();
        results::fetch(
            py,
            entities::io::state(py, connection)?,
            &compiled,
            if terminal == "one_opt" {
                "optional"
            } else {
                terminal
            },
        )
    }
}

#[pymethods]
impl PyPipeline {
    #[new]
    fn new(source: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: pipeline_source(source)?.pipeline(),
        })
    }

    fn filter(&self, condition: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .filter(PyPipelineExpr::coerce(condition)?.unbound()?),
        })
    }

    fn filter_with(&self, function: &Bound<'_, PyAny>) -> PyResult<Self> {
        let plan = callback(function, true)?;
        Ok(Self {
            inner: self.inner.clone().filter_with(|binder| {
                let [expression] = plan.lower(binder);
                expression
            }),
        })
    }

    #[pyo3(signature = (*columns))]
    fn derive(&self, columns: &Bound<'_, PyTuple>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().derive(expressions(columns)?),
        })
    }

    fn derive_with(&self, function: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: bound::apply(
                self.inner.clone(),
                Stage::Derive,
                callback(function, false)?,
            )?,
        })
    }

    #[pyo3(signature = (*columns))]
    fn select(&self, columns: &Bound<'_, PyTuple>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().select(expressions(columns)?),
        })
    }

    fn select_with(&self, function: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: bound::apply(
                self.inner.clone(),
                Stage::Select,
                callback(function, false)?,
            )?,
        })
    }

    #[pyo3(signature = (*keys))]
    fn group(&self, keys: &Bound<'_, PyTuple>) -> PyResult<PyGrouped> {
        Ok(PyGrouped {
            inner: self.inner.clone().group(expressions(keys)?),
        })
    }

    fn group_with(&self, function: &Bound<'_, PyAny>) -> PyResult<PyGrouped> {
        Ok(PyGrouped {
            inner: bound::grouped(self.inner.clone(), callback(function, false)?)?,
        })
    }

    #[pyo3(signature = (*columns, over))]
    fn window(&self, columns: &Bound<'_, PyTuple>, over: PyRef<'_, PyOver>) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .window(expressions(columns)?, over.inner.clone()),
        })
    }

    fn window_with(&self, over: PyRef<'_, PyOver>, function: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: bound::apply(
                self.inner.clone(),
                Stage::Window(over.inner.clone()),
                callback(function, false)?,
            )?,
        })
    }

    #[pyo3(signature = (*keys))]
    fn sort(&self, keys: &Bound<'_, PyTuple>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().sort(expressions(keys)?),
        })
    }

    fn sort_with(&self, function: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: bound::apply(self.inner.clone(), Stage::Sort, callback(function, false)?)?,
        })
    }

    fn take(&self, rows: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().take(integer(rows)?),
        })
    }

    fn take_range(&self, start: &Bound<'_, PyAny>, end: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .take_range(integer(start)?..=integer(end)?),
        })
    }

    #[pyo3(signature = (source, on, *, kind=Join::Inner))]
    fn join(&self, source: &Bound<'_, PyAny>, on: &Bound<'_, PyAny>, kind: Join) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().join(
                join_side(kind),
                pipeline_source(source)?.source(),
                PyPipelineExpr::coerce(on)?.unbound()?,
            ),
        })
    }

    #[pyo3(signature = (source, function, *, kind=Join::Inner))]
    fn join_with(
        &self,
        source: &Bound<'_, PyAny>,
        function: &Bound<'_, PyAny>,
        kind: Join,
    ) -> PyResult<Self> {
        let source = pipeline_source(source)?.source();
        let plan = callback(function, true)?;
        Ok(Self {
            inner: self
                .inner
                .clone()
                .join_with(join_side(kind), source, |binder| {
                    let [expression] = plan.lower(binder);
                    expression
                }),
        })
    }

    fn append(&self, source: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().append(pipeline_source(source)?.source()),
        })
    }

    fn intersect(&self, source: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: self
                .inner
                .clone()
                .intersect(pipeline_source(source)?.source()),
        })
    }

    fn remove(&self, source: &Bound<'_, PyAny>) -> PyResult<Self> {
        Ok(Self {
            inner: self.inner.clone().remove(pipeline_source(source)?.source()),
        })
    }

    fn distinct(&self) -> Self {
        Self {
            inner: self.inner.clone().distinct(),
        }
    }

    #[pyo3(signature = (*, terminal="all"))]
    fn inspect(&self, terminal: &str) -> PyResult<Compiled> {
        self.compile(terminal)
    }

    #[pyo3(signature = (selection, *, qualifiers=None))]
    fn select_sources(
        &self,
        selection: PyRef<'_, super::PySourceSelection>,
        qualifiers: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<super::PySelectedSources> {
        selection.select(self, qualifiers)
    }

    fn all<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.read(py, connection, "all")
    }

    fn one<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.read(py, connection, "one")
    }

    fn one_opt<'py>(
        &self,
        py: Python<'py>,
        connection: &Bound<'_, PyAny>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.read(py, connection, "one_opt")
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "pipelines cannot be tested as Python booleans",
        ))
    }
}

#[pyclass(
    name = "PipelineGrouped",
    module = "pgorm.pipeline",
    frozen,
    from_py_object
)]
#[derive(Clone, Debug)]
pub struct PyGrouped {
    inner: pl::Grouped,
}

#[pymethods]
impl PyGrouped {
    #[pyo3(signature = (*columns))]
    fn aggregate(&self, columns: &Bound<'_, PyTuple>) -> PyResult<PyPipeline> {
        Ok(PyPipeline {
            inner: self.inner.clone().aggregate(expressions(columns)?),
        })
    }

    fn aggregate_with(&self, function: &Bound<'_, PyAny>) -> PyResult<PyPipeline> {
        Ok(PyPipeline {
            inner: bound::aggregate(self.inner.clone(), callback(function, false)?)?,
        })
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "grouped pipelines require aggregation",
        ))
    }
}
