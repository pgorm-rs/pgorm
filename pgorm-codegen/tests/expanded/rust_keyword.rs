use pgorm::entity::prelude::*;

#[derive(Copy, Clone, Default, Debug, DeriveEntity)]
pub struct Entity;

impl EntityName for Entity {
    fn table_name(&self) -> &str {
        "rust_keyword"
    }
}

#[derive(Clone, Debug, PartialEq, DeriveModel, DeriveActiveModel, Eq)]
pub struct Model {
    pub id: i32,
    pub testing: i8,
    pub rust: u8,
    pub keywords: i16,
    pub r#type: u16,
    pub r#typeof: i32,
    pub crate_: u32,
    pub self_: i64,
    pub self_id1: u64,
    pub self_id2: i32,
    pub fruit_id1: i32,
    pub fruit_id2: i32,
    pub cake_id: i32,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
pub enum Column {
    Id,
    Testing,
    Rust,
    Keywords,
    Type,
    Typeof,
    Crate,
    Self_,
    SelfId1,
    SelfId2,
    FruitId1,
    FruitId2,
    CakeId,
}

#[derive(Copy, Clone, Debug, EnumIter, DerivePrimaryKey)]
pub enum PrimaryKey {
    Id,
}

impl PrimaryKeyTrait for PrimaryKey {
    type ValueType = i32;

    fn auto_increment() -> bool {
        true
    }
}

#[derive(Copy, Clone, Debug, EnumIter)]
pub enum Relation {
    SelfRef1,
    SelfRef2,
    Fruit1,
    Fruit2,
    Cake,
}

impl ColumnTrait for Column {
    type EntityName = Entity;

    fn def(&self) -> ColumnDef {
        match self {
            Self::Id => ColumnType::Integer.def(),
            Self::Testing => ColumnType::TinyInteger.def(),
            Self::Rust => ColumnType::TinyUnsigned.def(),
            Self::Keywords => ColumnType::SmallInteger.def(),
            Self::Type => ColumnType::SmallUnsigned.def(),
            Self::Typeof => ColumnType::Integer.def(),
            Self::Crate => ColumnType::Unsigned.def(),
            Self::Self_ => ColumnType::BigInteger.def(),
            Self::SelfId1 => ColumnType::BigUnsigned.def(),
            Self::SelfId2 => ColumnType::Integer.def(),
            Self::FruitId1 => ColumnType::Integer.def(),
            Self::FruitId2 => ColumnType::Integer.def(),
            Self::CakeId => ColumnType::Integer.def(),
        }
    }
}

impl RelationTrait for Relation {
    fn def(&self) -> RelationDef {
        match self {
            Self::SelfRef1 => Entity::belongs_to(Entity)
                .from(Column::SelfId1)
                .to(Column::Id)
                .into(),
            Self::SelfRef2 => Entity::belongs_to(Entity)
                .from(Column::SelfId2)
                .to(Column::Id)
                .into(),
            Self::Fruit1 => Entity::belongs_to(super::fruit::Entity)
                .from(Column::FruitId1)
                .to(super::fruit::Column::Id)
                .into(),
            Self::Fruit2 => Entity::belongs_to(super::fruit::Entity)
                .from(Column::FruitId2)
                .to(super::fruit::Column::Id)
                .into(),
            Self::Cake => Entity::belongs_to(super::cake::Entity)
                .from(Column::CakeId)
                .to(super::cake::Column::Id)
                .into(),
        }
    }
}

impl Related<super::cake::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Cake.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
