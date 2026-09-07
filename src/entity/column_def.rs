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

/// The enum type's cast spelling: `schema.name` when the type declares a
/// schema, the bare name otherwise — raw text either way, matching the
/// unquoted convention every enum cast renders under.
// [spec:pgorm:sem:entity.traits.column.enum-cast+3]
pub(crate) fn enum_cast_iden(col_type: &ColumnType) -> Option<DynIden> {
    match col_type {
        ColumnType::Enum { name, schema, .. } => Some(match schema {
            Some(schema) => {
                let qualified = format!("{}.{}", schema.to_string(), name.to_string());
                SharedIden::new(pgorm_query::Alias::new(qualified)) as DynIden
            }
            None => SharedIden::clone(name),
        }),
        ColumnType::Array(col_type) => enum_cast_iden(col_type),
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
        _ => match enum_cast_iden(col_type) {
            Some(enum_name) => f(expr, enum_name, col_type),
            None => expr.into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::{ColumnTrait, EntityTrait};

    // [spec:pgorm:sem:entity.traits.column.enum-cast+3/test]    every
    // value-position operand passes through `save_as` — between, if_null and
    // the array membership forms included — for the derive-generated override
    // and the enum default alike
    // [spec:pgorm:sem:entity.traits.column.enum-cast+3/test]    a
    // schema-qualified enum type reaches every rendering qualified: the value
    // cast, the array cast, the CREATE TABLE column type and CREATE TYPE
    #[test]
    #[cfg(feature = "macros")]
    fn schema_qualified_enum_renders_qualified_everywhere() {
        use crate::{QueryFilter, QueryTrait, Schema};

        mod housed {
            use crate as pgorm;
            use crate::entity::prelude::*;
            use crate::pgorm_query::{Alias, SharedIden};

            #[derive(Copy, Clone, Default, Debug, DeriveEntity)]
            pub struct Entity;

            impl EntityName for Entity {
                fn table_name(&self) -> &str {
                    "housed"
                }
            }

            #[derive(Clone, Debug, PartialEq, Eq, DeriveModel, DeriveActiveModel)]
            pub struct Model {
                pub id: i32,
                pub status: String,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
            pub enum Column {
                Id,
                Status,
            }

            #[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
            pub enum PrimaryKey {
                Id,
            }

            impl PrimaryKeyTrait for PrimaryKey {
                type ValueType = i32;

                fn auto_increment() -> bool {
                    false
                }
            }

            #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
            pub enum Relation {}

            impl ColumnTrait for Column {
                type EntityName = Entity;

                fn def(&self) -> ColumnDef {
                    match self {
                        Self::Id => ColumnType::Integer.def(),
                        Self::Status => ColumnType::Enum {
                            name: SharedIden::new(Alias::new("status")),
                            schema: Some(SharedIden::new(Alias::new("custom"))),
                            variants: vec![SharedIden::new(Alias::new("open"))],
                        }
                        .def(),
                    }
                }
            }

            impl ActiveModelBehavior for ActiveModel {}
        }

        assert_eq!(
            housed::Entity::find()
                .filter(housed::Column::Status.eq("open"))
                .as_query()
                .to_string(),
            [
                r#"SELECT "housed"."id", CAST("housed"."status" AS text)"#,
                r#"FROM "housed" WHERE "housed"."status" = (CAST('open' AS custom.status))"#,
            ]
            .join(" ")
        );
        assert_eq!(
            housed::Entity::find()
                .filter(housed::Column::Status.eq_any(["open".to_owned()]))
                .as_query()
                .to_string()
                .split_once("WHERE ")
                .expect("a WHERE clause")
                .1,
            r#""housed"."status" = ANY(CAST(ARRAY ['open'] AS custom.status[]))"#
        );

        let schema = Schema::new();
        assert_eq!(
            schema
                .create_enum_from_entity(housed::Entity)
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
            [r#"CREATE TYPE "custom"."status" AS ENUM ('open')"#]
        );
        assert!(
            schema
                .create_table_from_entity(housed::Entity)
                .to_string()
                .contains(r#""status" custom.status NOT NULL"#),
            "the column type renders qualified: {}",
            schema.create_table_from_entity(housed::Entity)
        );
    }
}
