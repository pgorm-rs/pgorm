use pgorm::entity::prelude::*;

use super::pgorm_active_enums::Mood;
use super::pgorm_active_enums::Tier;

#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
pub struct Entity;

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "survey"
    }
}

#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Eq)]
pub struct Model {
    pub id: i32,
    pub moods: Vec<Mood>,
    pub past_moods: Option<Vec<Mood>>,
    pub tiers: Vec<Tier>,
    pub past_tiers: Option<Vec<Tier>>,
    pub log: Option<Vec<Mood>>,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    Id,
    Moods,
    PastMoods,
    Tiers,
    PastTiers,
    Log,
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
            Self::Moods => ColumnType::Array(Arc::new(Mood::db_type().get_column_type().clone()))
                .def(),
            Self::PastMoods => ColumnType::Array(Arc::new(Mood::db_type().get_column_type().clone()))
                .def()
                .null(),
            Self::Tiers => ColumnType::Array(Arc::new(Tier::db_type().get_column_type().clone()))
                .def(),
            Self::PastTiers => ColumnType::Array(Arc::new(Tier::db_type().get_column_type().clone()))
                .def()
                .null(),
            Self::Log => ColumnType::Domain {
                name: Name::runtime("mood_log"),
                schema: None,
                base: Arc::new(ColumnType::Array(Arc::new(
                    Mood::db_type().get_column_type().clone(),
                ))),
            }
            .def()
            .null(),
        }
    }
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        panic!("No RelationDef")
    }
}

impl ActiveModelBehavior for ActiveModel {}
