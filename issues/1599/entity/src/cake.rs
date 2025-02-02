use pgorm::entity::prelude::*;

#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel)]
#[pgorm(table_name = "cake")]
pub struct Model {
    #[pgorm(primary_key)]
    pub id: i32,
    #[pgorm(column_name = "name", enum_name = "Name")]
    pub name: String,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[pgorm(has_many = "super::fruit::Entity")]
    Fruit,
    #[pgorm(
        has_many = "super::fruit::Entity",
        on_condition = r#"super::fruit::Column::Name.like("%tropical%")"#
    )]
    TropicalFruit,
    #[pgorm(
        has_many = "super::fruit::Entity",
        condition_type = "any",
        on_condition = r#"super::fruit::Column::Name.like("%tropical%")"#
    )]
    OrTropicalFruit,
}

impl Related<super::fruit::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::Fruit.def()
    }
}

impl Related<super::filling::Entity> for Entity {
    fn to() -> RelationDef {
        super::cake_filling::Relation::Filling.def()
    }

    fn via() -> Option<RelationDef> {
        Some(super::cake_filling::Relation::Cake.def().rev())
    }
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelatedEntity)]
pub enum RelatedEntity {
    #[pgorm(entity = "super::fruit::Entity")]
    Fruit,
    #[pgorm(entity = "super::filling::Entity")]
    Filling,
    #[pgorm(entity = "super::fruit::Entity", def = "Relation::TropicalFruit.def()")]
    TropicalFruit,
    #[pgorm(
        entity = "super::fruit::Entity",
        def = "Relation::OrTropicalFruit.def()"
    )]
    OrTropicalFruit,
}

impl ActiveModelBehavior for ActiveModel {}
