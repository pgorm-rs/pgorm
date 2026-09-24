//! Entities whose names are only known at run time, for the registry's
//! pgorm sites.

/// An entity whose table and schema names are computed per instance, the
/// pattern `EntityName::table_name(&self)` / `schema_name(&self)` exist for.
/// The name is leaked to `&'static str` so the entity stays `Copy`, which the
/// entity traits require; the oracle renders a few thousand statements, so
/// the leak is bounded.
pub mod dyn_named {
    use pgorm::entity::prelude::*;

    #[derive(Copy, Clone, Default, Debug, DeriveEntity)]
    pub struct Entity {
        pub table: &'static str,
        pub schema: Option<&'static str>,
    }

    impl Entity {
        /// The entity with `name` as its table.
        pub fn table(name: &str) -> Self {
            Self {
                table: leak(name),
                schema: None,
            }
        }

        /// The entity `t`, with `name` as its schema.
        pub fn schema(name: &str) -> Self {
            Self {
                table: "t",
                schema: Some(leak(name)),
            }
        }
    }

    fn leak(name: &str) -> &'static str {
        Box::leak(name.to_owned().into_boxed_str())
    }

    impl EntityName for Entity {
        fn table_name(&self) -> &str {
            self.table
        }

        fn schema_name(&self) -> Option<&str> {
            self.schema
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, DeriveModel, DeriveActiveModel)]
    pub struct Model {
        pub id: i32,
        pub name: String,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
    pub enum Column {
        Id,
        Name,
    }

    impl ColumnTrait for Column {
        type EntityName = Entity;

        fn def(&self) -> ColumnDef {
            match self {
                Column::Id => ColumnType::Integer.def(),
                Column::Name => ColumnType::Text.def().indexed(),
            }
        }
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

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

/// An entity with a hand-written `ColumnTrait::select_as` that casts its
/// column to a type named at run time — the read cast `select_sources`
/// projects through.
pub mod cast_named {
    use std::cell::RefCell;

    use pgorm::{
        entity::prelude::*,
        pgorm_query::{Expr, Name, SimpleExpr},
    };

    thread_local! {
        static TYPE: RefCell<String> = RefCell::new(String::from("text"));
    }

    /// Run `f` with the column's read cast naming `name`.
    pub fn with_type<T>(name: &str, f: impl FnOnce() -> T) -> T {
        TYPE.with(|cell| cell.replace(name.to_owned()));
        let out = f();
        TYPE.with(|cell| cell.replace(String::from("text")));
        out
    }

    #[derive(Copy, Clone, Default, Debug, DeriveEntity)]
    pub struct Entity;

    impl EntityName for Entity {
        fn table_name(&self) -> &str {
            "t"
        }
    }

    #[derive(Clone, Debug, PartialEq, Eq, DeriveModel, DeriveActiveModel)]
    pub struct Model {
        pub id: i32,
    }

    #[derive(Copy, Clone, Debug, EnumIter, DeriveColumn)]
    pub enum Column {
        Id,
    }

    impl ColumnTrait for Column {
        type EntityName = Entity;

        fn def(&self) -> ColumnDef {
            ColumnType::Integer.def()
        }

        fn select_as(&self, expr: Expr) -> SimpleExpr {
            expr.cast_as(TYPE.with(|cell| Name::runtime(cell.borrow().clone())))
        }
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

    #[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}
