use pgorm::pgorm_query::{Alias, ArrayType, TypeName, Value};
use pyo3::prelude::*;

use crate::errors::ConstructionError;

// [spec:pgorm:req:python.value-tags]
/// An owned, identifier-only PostgreSQL type name, optionally schema qualified.
#[pyclass(name = "TypeName", module = "pgorm", frozen, eq, from_py_object)]
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PyTypeName {
    #[pyo3(get)]
    pub name: String,
    #[pyo3(get)]
    pub schema: Option<String>,
}

#[pymethods]
impl PyTypeName {
    #[new]
    #[pyo3(signature = (name, *, schema=None))]
    fn new(name: &Bound<'_, PyAny>, schema: Option<&Bound<'_, PyAny>>) -> PyResult<Self> {
        let name = name
            .extract::<String>()
            .map_err(super::construction_error)?;
        let schema = schema
            .map(|schema| schema.extract::<String>())
            .transpose()
            .map_err(super::construction_error)?;
        for part in std::iter::once(&name).chain(schema.iter()) {
            if part.is_empty() || part.contains('\0') || part.len() > 63 {
                return Err(ConstructionError::new_err(
                    "type name parts must contain 1–63 UTF-8 bytes without NUL",
                ));
            }
        }
        Ok(Self { name, schema })
    }

    fn __repr__(&self) -> String {
        format!("TypeName({:?}, schema={:?})", self.name, self.schema)
    }
}

impl PyTypeName {
    /// Lower through Rust's qualified identifier constructor, never verbatim SQL.
    pub fn rust_type(&self) -> TypeName {
        let name = TypeName::new(Alias::new(&self.name));
        match &self.schema {
            Some(schema) => name.schema(Alias::new(schema)),
            None => name,
        }
    }
}

macro_rules! scalar_kinds {
    ($( $variant:ident => $name:literal ),+ $(,)?) => {
        pub const SCALAR_NAMES: &[&str] = &[$($name),+];

        pub fn scalar_name(kind: &ArrayType) -> &'static str {
            match kind { $(ArrayType::$variant => $name),+ }
        }

        pub fn parse_scalar(name: &str) -> PyResult<ArrayType> {
            match name {
                $($name => Ok(ArrayType::$variant),)+
                _ => Err(ConstructionError::new_err("unknown scalar value kind")),
            }
        }

        pub fn scalar_null(kind: &ArrayType) -> Value {
            match kind { $(ArrayType::$variant => Value::$variant(None)),+ }
        }

        pub fn rust_tag(value: &Value) -> Tag {
            match value {
                $(Value::$variant(_) => Tag::Scalar(ArrayType::$variant),)+
                Value::Array(kind, _) => Tag::Array(Box::new(Tag::Scalar(kind.clone()))),
            }
        }

        pub fn is_null(value: &Value) -> bool {
            match value {
                $(Value::$variant(value) => value.is_none(),)+
                Value::Array(_, value) => value.is_none(),
            }
        }
    };
}

scalar_kinds! {
    Bool => "bool", TinyInt => "i8", SmallInt => "i16", Int => "i32",
    BigInt => "i64", Unsigned => "u32", BigUnsigned => "u64",
    Float => "f32", Double => "f64", String => "text", Char => "char",
    Bytes => "bytes", Json => "json", Decimal => "decimal", Uuid => "uuid",
    ChronoDate => "date", ChronoTime => "time", ChronoDateTime => "datetime",
    ChronoDateTimeUtc => "datetime_utc", ChronoDateTimeLocal => "datetime_local",
    ChronoDateTimeWithTimeZone => "datetime_fixed", IpNetwork => "ipnetwork",
    MacAddress => "mac_address", Vector => "vector",
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Tag {
    Scalar(ArrayType),
    Enum(PyTypeName),
    Array(Box<Tag>),
}

impl Tag {
    pub fn parse(value: &Bound<'_, PyAny>) -> PyResult<Self> {
        if let Ok(name) = value.extract::<PyRef<'_, PyTypeName>>() {
            Ok(Self::Enum(name.clone()))
        } else {
            Ok(Self::Scalar(parse_scalar(value.extract::<&str>()?)?))
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Scalar(kind) => scalar_name(kind),
            Self::Enum(_) => "enum",
            Self::Array(_) => "array",
        }
    }

    pub fn array_type(&self) -> PyResult<ArrayType> {
        match self {
            Self::Scalar(kind) => Ok(kind.clone()),
            Self::Enum(_) => Ok(ArrayType::String),
            Self::Array(_) => Err(ConstructionError::new_err(
                "nested arrays are not supported",
            )),
        }
    }

    pub fn to_python(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self {
            Self::Enum(name) => Ok(Py::new(py, name.clone())?.into_any()),
            _ => Ok(self.name().into_pyobject(py)?.into_any().unbind()),
        }
    }
}
