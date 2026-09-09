use pgorm::pgorm_query::{ColumnType, Value};
use pgorm::{ColumnTrait, EntityTrait, IdenStr, Iterable, PrimaryKeyToColumn};
use pyo3::{prelude::*, types::PyString};
use serde_json::{Value as Json, json};

use crate::{
    errors::{ConstructionError, DecodeError},
    identifiers::validate_name,
    values::{PyTypeName, PyValue},
};

#[derive(Clone, Debug)]
pub(crate) enum InputKind {
    Scalar(&'static str),
    Enum(PyTypeName),
    Array(Box<Self>),
    Explicit,
}

impl InputKind {
    fn from_column(ty: &ColumnType) -> Self {
        let scalar = match ty {
            ColumnType::Char(Some(1)) => "char",
            ColumnType::Char(_) | ColumnType::String(_) | ColumnType::Text => "text",
            ColumnType::Bytea => "bytes",
            ColumnType::SmallInteger => "i16",
            ColumnType::Integer => "i32",
            ColumnType::BigInteger => "i64",
            ColumnType::Float => "f32",
            ColumnType::Double => "f64",
            ColumnType::Decimal(_) => "decimal",
            ColumnType::Boolean => "bool",
            ColumnType::Date => "date",
            ColumnType::Time => "time",
            ColumnType::Timestamp => "datetime",
            ColumnType::TimestampWithTimeZone => "datetime_utc",
            ColumnType::Json | ColumnType::JsonBinary => "json",
            ColumnType::Uuid => "uuid",
            ColumnType::Vector(_) => "vector",
            ColumnType::Inet | ColumnType::Cidr => "ipnetwork",
            ColumnType::MacAddr => "mac_address",
            ColumnType::Enum { name, schema, .. } => {
                return Self::Enum(PyTypeName {
                    name: name.to_string(),
                    schema: schema.as_ref().map(|s| s.to_string()),
                });
            }
            ColumnType::Array(member) => return Self::Array(Box::new(Self::from_column(member))),
            _ => return Self::Explicit,
        };
        Self::Scalar(scalar)
    }

    fn kind<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        match self {
            Self::Scalar(name) => Ok(PyString::new(py, name).into_any()),
            Self::Enum(name) => Ok(Py::new(py, name.clone())?.into_bound(py).into_any()),
            _ => Err(ConstructionError::new_err(
                "this column requires an explicit tagged Value",
            )),
        }
    }

    pub(crate) fn coerce(&self, value: &Bound<'_, PyAny>) -> PyResult<PyValue> {
        if let Ok(value) = value.extract::<PyRef<'_, PyValue>>() {
            if let Some(cast) = value.enum_cast() {
                let (expected, array) = match self {
                    Self::Enum(name) => (Some(name), false),
                    Self::Array(member) => (
                        match member.as_ref() {
                            Self::Enum(name) => Some(name),
                            _ => None,
                        },
                        true,
                    ),
                    _ => (None, false),
                };
                if !expected.is_some_and(|name| {
                    cast.name.to_string() == name.name
                        && cast.schema.as_ref().map(|s| s.to_string()) == name.schema
                        && cast.array == array
                }) {
                    return Err(ConstructionError::new_err(
                        "enum Value belongs to a different declared column type",
                    ));
                }
            }
            return Ok(value.clone());
        }
        let py = value.py();
        let class = py.get_type::<PyValue>();
        let result = match self {
            Self::Array(member) => class.call_method1("array", (member.kind(py)?, value))?,
            _ => class.call1((value, self.kind(py)?))?,
        };
        Ok(result.extract()?)
    }

    pub(crate) fn tagged(&self, value: Value) -> PyResult<PyValue> {
        match self {
            Self::Enum(name) if matches!(value, Value::String(_)) => {
                Ok(PyValue::from_enum(value, name.clone(), false))
            }
            Self::Array(member) => match member.as_ref() {
                Self::Enum(name)
                    if matches!(
                        value,
                        Value::Array(pgorm::pgorm_query::ArrayType::String, _)
                    ) =>
                {
                    Ok(PyValue::from_enum(value, name.clone(), true))
                }
                Self::Enum(_) => Err(DecodeError::new_err(
                    "compiled enum array returned an incompatible Rust Value",
                )),
                _ => Ok(PyValue::from_rust(value)),
            },
            Self::Enum(_) => Err(DecodeError::new_err(
                "compiled enum returned an incompatible Rust Value",
            )),
            _ => Ok(PyValue::from_rust(value)),
        }
    }

    fn describe(&self) -> Json {
        match self {
            Self::Scalar(name) => json!({"kind": name}),
            Self::Enum(name) => json!({"kind": "enum", "name": name.name, "schema": name.schema}),
            Self::Array(member) => json!({"kind": "array", "element": member.describe()}),
            Self::Explicit => json!({"kind": "explicit_value_required"}),
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ColumnInfo {
    pub(crate) name: String,
    pub(crate) json_key: String,
    pub(crate) sql_type: String,
    pub(crate) nullable: bool,
    pub(crate) primary_key: bool,
    pub(crate) input: InputKind,
}

impl ColumnInfo {
    pub(crate) fn describe(&self) -> Json {
        json!({"name": self.name, "json_key": self.json_key, "sql_type": self.sql_type,
            "nullable": self.nullable, "primary_key": self.primary_key, "input_hint": self.input.describe()})
    }
}

#[derive(Debug)]
pub(crate) struct EntityInfo {
    pub(crate) name: String,
    pub(crate) schema: Option<String>,
    pub(crate) table: String,
    pub(crate) entity_type: &'static str,
    pub(crate) column_type: &'static str,
    pub(crate) model_type: &'static str,
    pub(crate) active_type: &'static str,
    pub(crate) columns: Vec<ColumnInfo>,
}

impl EntityInfo {
    pub(crate) fn of<E: EntityTrait>(name: &str) -> PyResult<Self> {
        if name.is_empty() || name.len() > 255 || name.contains('\0') {
            return Err(ConstructionError::new_err(
                "registration names require 1–255 UTF-8 bytes without NUL",
            ));
        }
        let entity = E::default();
        let table = entity.table_name().to_owned();
        let schema = entity.schema_name().map(str::to_owned);
        validate_name(&table)?;
        if let Some(schema) = &schema {
            validate_name(schema)?;
        }
        let keys: Vec<_> = E::PrimaryKey::iter()
            .map(|k| k.into_column().as_str().to_owned())
            .collect();
        let mut names = std::collections::HashSet::new();
        let mut columns = Vec::new();
        for column in E::Column::iter() {
            let name = column.as_str().to_owned();
            validate_name(&name)?;
            if !names.insert(name.clone()) {
                return Err(ConstructionError::new_err(
                    "compiled entity has duplicate SQL column names",
                ));
            }
            let definition = column.def();
            columns.push(ColumnInfo {
                primary_key: keys.contains(&name),
                name,
                json_key: column.json_key().to_owned(),
                nullable: definition.is_null(),
                sql_type: format!("{:?}", definition.get_column_type()),
                input: InputKind::from_column(definition.get_column_type()),
            });
        }
        if columns.is_empty() {
            return Err(ConstructionError::new_err(
                "compiled entities require a nonempty projection",
            ));
        }
        Ok(Self {
            name: name.to_owned(),
            schema,
            table,
            columns,
            entity_type: std::any::type_name::<E>(),
            column_type: std::any::type_name::<E::Column>(),
            model_type: std::any::type_name::<E::Model>(),
            active_type: std::any::type_name::<E::ActiveModel>(),
        })
    }

    pub(crate) fn column(&self, name: &str) -> PyResult<&ColumnInfo> {
        self.columns
            .iter()
            .find(|c| c.name == name)
            .ok_or_else(|| ConstructionError::new_err("unknown compiled entity column"))
    }

    pub(crate) fn describe(&self) -> Json {
        json!({"name": self.name, "schema": self.schema, "table": self.table,
            "rust_entity": self.entity_type, "rust_column": self.column_type, "rust_model": self.model_type, "rust_active_model": self.active_type,
            "columns": self.columns.iter().map(ColumnInfo::describe).collect::<Vec<_>>(),
            "terminals": ["all", "one", "one_opt", "active.insert", "active.update", "active.delete"],
            "active_states": ["not_set", "set", "unchanged"], "hooks": "Rust ActiveModelBehavior"})
    }
}
