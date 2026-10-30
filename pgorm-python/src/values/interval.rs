//! `pgorm.Interval`: PostgreSQL's `interval` as an immutable Python object.
//!
//! `datetime.timedelta` is not the shape: it has no months, and it normalises
//! its fields so that only the days may be negative, which would turn
//! `-1 hour` into `-1 day +23 hours`. The class keeps the three fields as
//! pgorm's `Interval` does, each with its own sign.

use pgorm::pgorm_query::Interval;
use pyo3::{prelude::*, types::PyInt};

use crate::errors::ConstructionError;

/// Months, days and microseconds, each independently signed.
// [spec:pgorm:req:bindings.values]
// [spec:pgorm:req:python.values+6]
#[pyclass(name = "Interval", module = "pgorm", frozen, eq, hash, from_py_object)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PyInterval(pub(crate) Interval);

/// An exact Python `int` within `i64`, refusing a `bool`, which is an `int`
/// subclass, and anything that only converts to one.
fn field(value: &Bound<'_, PyAny>, name: &str) -> PyResult<i64> {
    if !value.is_exact_instance_of::<PyInt>() {
        return Err(ConstructionError::new_err(format!(
            "an interval's {name} must be an int"
        )));
    }
    value.extract::<i64>().map_err(|_| out_of_range(name))
}

fn out_of_range(name: &str) -> PyErr {
    ConstructionError::new_err(format!("an interval's {name} is out of range"))
}

fn narrow(value: i64, name: &str) -> PyResult<i32> {
    i32::try_from(value).map_err(|_| out_of_range(name))
}

#[pymethods]
impl PyInterval {
    #[new]
    #[pyo3(signature = (months=None, days=None, microseconds=None))]
    fn new(
        months: Option<&Bound<'_, PyAny>>,
        days: Option<&Bound<'_, PyAny>>,
        microseconds: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let read = |value: Option<&Bound<'_, PyAny>>, name| {
            value.map_or(Ok(0), |value| field(value, name))
        };
        Ok(Self(Interval::new(
            narrow(read(months, "months")?, "months")?,
            narrow(read(days, "days")?, "days")?,
            read(microseconds, "microseconds")?,
        )))
    }

    #[getter]
    fn months(&self) -> i32 {
        self.0.months
    }

    #[getter]
    fn days(&self) -> i32 {
        self.0.days
    }

    #[getter]
    fn microseconds(&self) -> i64 {
        self.0.microseconds
    }

    /// The text PostgreSQL's interval input reads back as this value.
    fn __str__(&self) -> String {
        self.0.to_string()
    }

    fn __repr__(&self) -> String {
        format!(
            "Interval(months={}, days={}, microseconds={})",
            self.0.months, self.0.days, self.0.microseconds
        )
    }
}
