//! Column-type introspection and [`ColumnDef`] construction.

use super::*;

/// pgorm's utility methods that act on [ColumnType]
pub trait ColumnTypeTrait {
    /// Instantiate a new [ColumnDef]
    fn def(self) -> ColumnDef;

    /// Get the name of the enum if this is a enum column
    fn get_enum_name(&self) -> Option<&DynIden>;
}

// [spec:pgorm:req:entity.traits.column-def]
impl ColumnTypeTrait for ColumnType {
    fn def(self) -> ColumnDef {
        ColumnDef {
            col_type: self,
            null: false,
            unique: false,
            indexed: false,
            default: None,
            comment: None,
        }
    }

    fn get_enum_name(&self) -> Option<&DynIden> {
        enum_name(self)
    }
}

impl ColumnTypeTrait for ColumnDef {
    fn def(self) -> ColumnDef {
        self
    }

    fn get_enum_name(&self) -> Option<&DynIden> {
        enum_name(&self.col_type)
    }
}

fn enum_name(col_type: &ColumnType) -> Option<&DynIden> {
    match col_type {
        ColumnType::Enum { name, .. } => Some(name),
        ColumnType::Array(col_type) => enum_name(col_type),
        _ => None,
    }
}

impl ColumnDef {
    /// Marks the column as `UNIQUE`
    pub fn unique(mut self) -> Self {
        self.unique = true;
        self
    }
    /// Set column comment
    pub fn comment(mut self, v: &str) -> Self {
        self.comment = Some(v.into());
        self
    }

    /// Mark the column as nullable
    pub fn null(self) -> Self {
        self.nullable()
    }

    /// Mark the column as nullable
    pub fn nullable(mut self) -> Self {
        self.null = true;
        self
    }

    /// Set the `indexed` field  to `true`
    pub fn indexed(mut self) -> Self {
        self.indexed = true;
        self
    }

    /// Set the default value
    pub fn default_value<T>(mut self, value: T) -> Self
    where
        T: Into<Value>,
    {
        self.default = Some(value.into().into());
        self
    }

    /// Set the default value or expression of a column
    pub fn default<T>(mut self, default: T) -> Self
    where
        T: Into<SimpleExpr>,
    {
        self.default = Some(default.into());
        self
    }

    /// Get [ColumnType] as reference
    pub fn get_column_type(&self) -> &ColumnType {
        &self.col_type
    }

    /// Returns true if the column is nullable
    pub fn is_null(&self) -> bool {
        self.null
    }
}

pub(crate) fn cast_enum_as<C, F>(expr: Expr, col: &C, f: F) -> SimpleExpr
where
    C: ColumnTrait,
    F: Fn(Expr, DynIden, &ColumnType) -> SimpleExpr,
{
    let col_def = col.def();
    let col_type = col_def.get_column_type();

    match col_type {
        #[cfg(all(feature = "with-json", feature = "postgres-array"))]
        ColumnType::Json | ColumnType::JsonBinary => {
            use pgorm_query::ArrayType;
            use serde_json::Value as Json;

            #[allow(clippy::boxed_local)]
            fn unbox<T>(boxed: Box<T>) -> T {
                *boxed
            }

            let expr = expr.into();
            match expr {
                SimpleExpr::Value(Value::Array(ArrayType::Json, Some(json_vec))) => {
                    // flatten Array(Vec<Json>) into Json
                    let json_vec: Vec<Json> = json_vec
                        .into_iter()
                        .filter_map(|val| match val {
                            Value::Json(Some(json)) => Some(unbox(json)),
                            _ => None,
                        })
                        .collect();
                    SimpleExpr::Value(Value::Json(Some(Box::new(json_vec.into()))))
                }
                SimpleExpr::Value(Value::Array(ArrayType::Json, None)) => {
                    SimpleExpr::Value(Value::Json(None))
                }
                _ => expr,
            }
        }
        _ => match col_type.get_enum_name() {
            Some(enum_name) => f(expr, SeaRc::clone(enum_name), col_type),
            None => expr.into(),
        },
    }
}
