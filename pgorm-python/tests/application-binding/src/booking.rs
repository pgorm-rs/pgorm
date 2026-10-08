use pgorm::entity::prelude::*;

/// A range type the application's schema creates over `float8`.
#[derive(Clone, Debug, PartialEq, DeriveCreatedRange)]
#[pgorm(range_name = "floatrange", schema_name = "python_entities")]
pub struct FloatRange(pub Range<f64>);

/// The multirange PostgreSQL creates beside it.
#[derive(Clone, Debug, PartialEq, DeriveCreatedRange)]
#[pgorm(multirange_name = "floatmultirange", schema_name = "python_entities")]
pub struct FloatSpans(pub Multirange<f64>);

/// A booking over a created range and its multirange, registered so that
/// Python reads and writes a type only the schema names.
#[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
#[pgorm(table_name = "bookings", schema_name = "python_entities")]
pub struct Model {
    #[pgorm(primary_key, auto_increment = false)]
    pub id: i32,
    pub span: FloatRange,
    pub spans: Option<FloatSpans>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
