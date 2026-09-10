//! Closed runtime type constructors over pgorm_query::ColumnType.

use crate::{UnsupportedCapabilityError, errors::ConstructionError, values::PyTypeName};
use pgorm::pgorm_query::{Alias, ColumnType, IntervalSpec, IntoIden, StringLen};
use pyo3::{prelude::*, types::PyInt};
use std::sync::Arc;

#[derive(Clone, Debug)]
#[pyclass(name = "DataType", module = "pgorm.schema", frozen, from_py_object)]
pub struct PyDataType {
    pub(crate) inner: ColumnType,
}

fn integer(value: Option<&Bound<'_, PyAny>>, label: &str) -> PyResult<Option<u32>> {
    value
        .map(|value| {
            if !value.is_exact_instance_of::<PyInt>() {
                return Err(ConstructionError::new_err(format!(
                    "{label} requires an exact integer"
                )));
            }
            value
                .extract::<u32>()
                .map_err(|_| ConstructionError::new_err(format!("{label} is outside u32")))
        })
        .transpose()
}

impl PyDataType {
    pub(super) fn coerce(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(kind) = value.extract::<PyRef<'_, Self>>() {
            return Ok(kind.clone());
        }
        Self::new(value, None, None, None)
    }
}

#[pymethods]
impl PyDataType {
    // [spec:pgorm:req:python.schema]
    #[new]
    #[pyo3(signature=(kind, *, length=None, precision=None, scale=None))]
    fn new(
        kind: &Bound<'_, PyAny>,
        length: Option<&Bound<'_, PyAny>>,
        precision: Option<&Bound<'_, PyAny>>,
        scale: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<Self> {
        let length = integer(length, "length")?;
        let precision = integer(precision, "precision")?;
        let scale = integer(scale, "scale")?;
        if length == Some(0) {
            return Err(ConstructionError::new_err("type length must be positive"));
        }
        if let Ok(name) = kind.extract::<PyRef<'_, PyTypeName>>() {
            if length.is_some() || precision.is_some() || scale.is_some() {
                return Err(ConstructionError::new_err(
                    "named types do not accept size modifiers",
                ));
            }
            return Ok(Self {
                inner: ColumnType::Enum {
                    name: Alias::new(&name.name).into_iden(),
                    schema: name
                        .schema
                        .as_ref()
                        .map(|schema| Alias::new(schema).into_iden()),
                    variants: Vec::new(),
                },
            });
        }
        let kind = kind.extract::<&str>().map_err(|_| {
            ConstructionError::new_err("DataType requires a supported type name or TypeName")
        })?;
        if length.is_some() && !matches!(kind, "char" | "varchar" | "bit" | "varbit" | "vector") {
            return Err(ConstructionError::new_err(
                "this type does not accept length",
            ));
        }
        if (precision.is_some() || scale.is_some()) && kind != "numeric" {
            return Err(ConstructionError::new_err(
                "precision and scale require numeric",
            ));
        }
        let inner = match kind {
            "char" => ColumnType::Char(length),
            "varchar" => ColumnType::String(length.map_or(StringLen::None, StringLen::N)),
            "text" => ColumnType::Text,
            "smallint" => ColumnType::SmallInteger,
            "integer" => ColumnType::Integer,
            "bigint" => ColumnType::BigInteger,
            "real" => ColumnType::Float,
            "double" => ColumnType::Double,
            "numeric" => {
                let size = match (precision, scale) {
                    (None, None) => None,
                    (Some(p @ 1..=1000), s) if s.unwrap_or(0) <= 1000 => Some((p, s.unwrap_or(0))),
                    _ => {
                        return Err(ConstructionError::new_err(
                            "numeric requires precision 1–1000 and optional scale 0–1000",
                        ));
                    }
                };
                ColumnType::Decimal(size)
            }
            "boolean" => ColumnType::Boolean,
            "date" => ColumnType::Date,
            "time" => ColumnType::Time,
            "timestamp" => ColumnType::Timestamp,
            "timestamptz" => ColumnType::TimestampWithTimeZone,
            "interval" => ColumnType::Interval(IntervalSpec::Any(None)),
            "bytea" => ColumnType::Bytea,
            "bit" => ColumnType::Bit(length),
            "varbit" => ColumnType::VarBit(
                length.ok_or_else(|| ConstructionError::new_err("varbit requires length"))?,
            ),
            "money" => ColumnType::Money,
            "json" => ColumnType::Json,
            "jsonb" => ColumnType::JsonBinary,
            "uuid" => ColumnType::Uuid,
            "vector" => ColumnType::Vector(length),
            "cidr" => ColumnType::Cidr,
            "inet" => ColumnType::Inet,
            "macaddr" => ColumnType::MacAddr,
            "ltree" => ColumnType::LTree,
            _ => {
                return Err(UnsupportedCapabilityError::new_err(
                    "unsupported DDL column type",
                ));
            }
        };
        Ok(Self { inner })
    }

    fn array(&self) -> Self {
        Self {
            inner: ColumnType::Array(Arc::new(self.inner.clone())),
        }
    }

    fn __repr__(&self) -> String {
        format!("DataType({:?})", self.inner)
    }
}
