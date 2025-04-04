use pgorm::entity::prelude::*;

#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
pub struct Entity;

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "parent"
    }
}

#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Eq)]
pub struct Model {
    pub id1: i32,
    pub id2: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    Id1,
    Id2,
}

#[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
pub enum PrimaryKey {
    Id1,
    Id2,
}

impl PrimaryKeyTrait for PrimaryKey {
    type ValueType = (i32, i32);
    fn auto_increment() -> bool {
        false
    }
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    Child,
}

impl ColumnTrait for Column {
    type EntityName = Entity;
    fn def(&self) -> ColumnDef {
        match self {
            Self::Id1 => ColumnType::Integer.def(),
            Self::Id2 => ColumnType::Integer.def(),
        }
    }
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::Child => Entity::has_many(super::child::Entity).into(),
        }
    }
}

impl Related<super::child::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Child.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
