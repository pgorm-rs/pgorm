use pgorm::entity::prelude::*;

use super::pgorm_active_enums::Mood;
use super::pgorm_range_types::Floatrange;

#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
pub struct Entity;

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "stock_line"
    }
}

#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel)]
pub struct Model {
    pub id: i32,
    pub qty: Option<i32>,
    pub small: i32,
    pub code: String,
    pub codes: Vec<String>,
    pub mood: Mood,
    pub moods: Option<Vec<Mood>>,
    pub labels: Option<Vec<String>>,
    pub span: Option<Floatrange>,
    pub amount: Decimal,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    Id,
    Qty,
    Small,
    Code,
    Codes,
    Mood,
    Moods,
    Labels,
    Span,
    Amount,
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

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {}

impl ColumnTrait for Column {
    type EntityName = Entity;
    fn def(&self) -> ColumnDef {
        match self {
            Self::Id => ColumnType::Integer.def(),
            Self::Qty => ColumnType::Domain {
                name: Name::runtime("quantity"),
                schema: None,
                base: Arc::new(ColumnType::Integer),
            }
            .def()
            .null(),
            Self::Small => ColumnType::Domain {
                name: Name::runtime("small_quantity"),
                schema: None,
                base: Arc::new(ColumnType::Domain {
                    name: Name::runtime("quantity"),
                    schema: None,
                    base: Arc::new(ColumnType::Integer),
                }),
            }
            .def(),
            Self::Code => ColumnType::Domain {
                name: Name::runtime("code"),
                schema: None,
                base: Arc::new(ColumnType::String(StringLen::N(8u32))),
            }
            .def(),
            Self::Codes => ColumnType::Array(Arc::new(ColumnType::Domain {
                name: Name::runtime("code"),
                schema: None,
                base: Arc::new(ColumnType::String(StringLen::N(8u32))),
            }))
            .def(),
            Self::Mood => ColumnType::Domain {
                name: Name::runtime("cheerful"),
                schema: None,
                base: Arc::new(Mood::db_type().get_column_type().clone()),
            }
            .def(),
            Self::Moods => ColumnType::Array(Arc::new(ColumnType::Domain {
                name: Name::runtime("cheerful"),
                schema: None,
                base: Arc::new(Mood::db_type().get_column_type().clone()),
            }))
            .def()
            .null(),
            Self::Labels => ColumnType::Domain {
                name: Name::runtime("labels"),
                schema: None,
                base: Arc::new(ColumnType::Array(Arc::new(ColumnType::Text))),
            }
            .def()
            .null(),
            Self::Span => ColumnType::Domain {
                name: Name::runtime("span"),
                schema: None,
                base: Arc::new(ColumnType::CreatedRange {
                    name: Name::runtime("floatrange"),
                    schema: None,
                    subtype: Arc::new(ColumnType::Double),
                }),
            }
            .def()
            .null(),
            Self::Amount => ColumnType::Domain {
                name: Name::runtime("amount"),
                schema: Some(Name::runtime("billing")),
                base: Arc::new(ColumnType::Decimal(Some((12u32, 2u32)))),
            }
            .def(),
        }
    }
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        panic!("No RelationDef")
    }
}

impl ActiveModelBehavior for ActiveModel {}
