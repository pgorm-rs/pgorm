//! Column-type introspection and [`ColumnDef`] construction.

use super::*;
use pgorm_query::{GeneratedKind, IdentityGeneration};

/// pgorm's utility methods that act on [ColumnType]
pub trait ColumnTypeTrait {
    /// Instantiate a new [ColumnDef]
    fn def(self) -> ColumnDef;

    /// Get the name of the enum if this is a enum column
    fn get_enum_name(&self) -> Option<&Name>;
}

// [spec:pgorm:req:entity.traits.column-def+2]
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

    fn get_enum_name(&self) -> Option<&Name> {
        enum_name(self)
    }
}

impl ColumnTypeTrait for ColumnDef {
    fn def(self) -> ColumnDef {
        self
    }

    fn get_enum_name(&self) -> Option<&Name> {
        enum_name(&self.col_type)
    }
}

fn enum_name(col_type: &ColumnType) -> Option<&Name> {
    match col_type {
        ColumnType::Enum { name, .. } => Some(name),
        ColumnType::Array(col_type) => enum_name(col_type),
        _ => None,
    }
}

/// The `LIKE`-metacharacter escape behind the substring sugar: `%`, `_` and
/// the escape character itself become literal, so search text matches
/// itself and nothing else.
// [spec:pgorm:def:entity.traits.column+6]
pub(crate) fn escape_like_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// The enum type a column stores, as the structured [`TypeName`] every cast
/// renders — schema qualification carried, the array flag set for a column
/// holding an array of the enum.
// [spec:pgorm:sem:entity.traits.column.enum-cast+4]
pub(crate) fn enum_type_name(col_type: &ColumnType) -> Option<pgorm_query::TypeName> {
    match col_type {
        ColumnType::Enum { name, schema, .. } => {
            let type_name = pgorm_query::TypeName::new(Name::clone(name));
            Some(match schema {
                Some(schema) => type_name.schema(Name::clone(schema)),
                None => type_name,
            })
        }
        ColumnType::Array(col_type) => Some(enum_type_name(col_type)?.array()),
        _ => None,
    }
}

/// What fills a column an insert leaves out: a `DEFAULT` expression, the
/// sequence of an identity column, or the expression of a generated one.
///
/// The three share one slot because PostgreSQL holds them to one: a column
/// declaring any two is refused (`42601`, "both default and identity
/// specified", "both default and generation expression specified", "both
/// identity and generation expression specified"), so a definition carrying
/// two has no table to describe.
// [spec:pgorm:req:entity.traits.column-def+2]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ColumnDefault {
    /// `DEFAULT <expr>`.
    Expr(SimpleExpr),
    /// `GENERATED { ALWAYS | BY DEFAULT } AS IDENTITY`.
    Identity(IdentityGeneration),
    /// `GENERATED ALWAYS AS (<expr>) { STORED | VIRTUAL }`.
    Generated(SimpleExpr, GeneratedKind),
}

// [spec:pgorm:req:entity.traits.column-def+2]
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

    /// Set the default value, replacing an identity or a generation expression
    pub fn default_value<T>(mut self, value: T) -> Self
    where
        T: Into<Value>,
    {
        self.default = Some(ColumnDefault::Expr(value.into().into()));
        self
    }

    /// Set the default value or expression of a column, replacing an identity
    /// or a generation expression
    pub fn default<T>(mut self, default: T) -> Self
    where
        T: Into<SimpleExpr>,
    {
        self.default = Some(ColumnDefault::Expr(default.into()));
        self
    }

    /// Generate the column's values `ALWAYS` — `GENERATED ALWAYS AS
    /// IDENTITY` — replacing a default or a generation expression.
    ///
    /// An insert that leaves the column out takes the sequence's next value,
    /// and one that names it is refused (`428C9`), as is an update that sets
    /// it. This is how a generated column inside a composite primary key is
    /// declared — `id` in `(tenant_id, id)` — since the serial family
    /// [`auto_increment`](crate::PrimaryKeyTrait::auto_increment) draws from
    /// belongs to a one-column key alone.
    pub fn identity(mut self) -> Self {
        self.default = Some(ColumnDefault::Identity(IdentityGeneration::Always));
        self
    }

    /// Generate the column's values `BY DEFAULT` — `GENERATED BY DEFAULT AS
    /// IDENTITY` — replacing a default or a generation expression: an insert
    /// that leaves the column out takes the sequence's next value, and one that
    /// names it keeps the value it names.
    pub fn identity_by_default(mut self) -> Self {
        self.default = Some(ColumnDefault::Identity(IdentityGeneration::ByDefault));
        self
    }

    /// Compute the column from the others in its row —
    /// `GENERATED ALWAYS AS (<expr>) { STORED | VIRTUAL }` — replacing a
    /// default or an identity.
    ///
    /// A generated column is read like any other and written by nobody: an
    /// insert or update that supplies a value for it is refused (`428C9`), as
    /// one naming a `GENERATED ALWAYS` identity column is, so a model writes
    /// it by leaving it `NotSet`. A [`Virtual`](GeneratedKind::Virtual) one
    /// cannot be `unique` or `indexed`, which PostgreSQL refuses (`0A000`) when
    /// the schema is created.
    pub fn generated<T>(mut self, expr: T, kind: GeneratedKind) -> Self
    where
        T: Into<SimpleExpr>,
    {
        self.default = Some(ColumnDefault::Generated(expr.into(), kind));
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
    F: Fn(Expr, pgorm_query::TypeName, &ColumnType) -> SimpleExpr,
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
        _ => match enum_type_name(col_type) {
            Some(type_name) => f(expr, type_name, col_type),
            None => expr.into(),
        },
    }
}

#[cfg(test)]
mod tests {
    use crate::{ColumnTrait, EntityTrait};

    // [spec:pgorm:sem:entity.traits.column.enum-cast+4/test]    every
    // value-position operand passes through `save_as` — between, if_null and
    // the array membership forms included — for the derive-generated override
    // and the enum default alike
    // [spec:pgorm:sem:entity.traits.column.enum-cast+4/test]    a
    // schema-qualified enum type reaches every rendering qualified: the value
    // cast, the array cast, the CREATE TABLE column type and CREATE TYPE
    #[test]
    #[cfg(feature = "macros")]
    fn schema_qualified_enum_renders_qualified_everywhere() {
        use crate::{QueryFilter, QueryTrait, Schema};

        mod housed {
            use crate as pgorm;
            use crate::entity::prelude::*;
            use crate::pgorm_query::Name;

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
                            name: Name::runtime("status"),
                            schema: Some(Name::runtime("custom")),
                            variants: vec![Name::runtime("open")],
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
                r#"FROM "housed" WHERE "housed"."status" = CAST('open' AS custom.status)"#,
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
