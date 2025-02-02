pub mod prelude {
    pub use super::model::{
        ActiveModel as ContentActiveModel, Column as ContentColumn, Entity as Content,
        Model as ContentModel, PrimaryKey as ContentPrimaryKey, Relation as ContentRelation,
    };
}

pub mod model {
    use pgorm::entity::prelude::*;

    #[derive(Clone, Debug, PartialEq, DeriveEntityModel)]
    #[pgorm(table_name = "content")]
    pub struct Model {
        #[pgorm(primary_key)]
        pub id: i32,
        pub container_id: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {
        #[pgorm(
            belongs_to = "crate::Container",
            from = "crate::ContentColumn::ContainerId",
            to = "crate::ContainerColumn::RustId"
        )]
        Container, // 1(Container) ⇆ n(Content)
    }

    impl Related<crate::Container> for Entity {
        fn to() -> RelationDef {
            Relation::Container.def()
        }
    }

    impl ActiveModelBehavior for ActiveModel {}
}
