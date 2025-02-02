#![allow(unused_imports, dead_code)]

use entity::{Column, Entity};
use pgorm::{ColumnTrait, DerivePartialModel, EntityTrait, FromQueryResult, ModelTrait};
use pgorm_query::Expr;

mod entity {
    use pgorm::prelude::*;

    #[derive(Debug, Clone, DeriveEntityModel)]
    #[pgorm(table_name = "foo_table")]
    pub struct Model {
        #[pgorm(primary_key)]
        id: i32,
        foo: i32,
        bar: String,
        foo2: bool,
        bar2: f64,
    }

    #[derive(Debug, DeriveRelation, EnumIter)]
    pub enum Relation {}

    impl ActiveModelBehavior for ActiveModel {}
}

#[derive(FromQueryResult, DerivePartialModel)]
#[pgorm(entity = "Entity")]
struct SimpleTest {
    _foo: i32,
    _bar: String,
}

#[derive(FromQueryResult, DerivePartialModel)]
#[pgorm(entity = "<entity::Model as ModelTrait>::Entity")]
struct EntityNameNotAIdent {
    #[pgorm(from_col = "foo2")]
    _foo: i32,
    #[pgorm(from_col = "bar2")]
    _bar: String,
}

#[derive(FromQueryResult, DerivePartialModel)]
#[pgorm(entity = "Entity")]
struct FieldFromDiffNameColumnTest {
    #[pgorm(from_col = "foo2")]
    _foo: i32,
    #[pgorm(from_col = "bar2")]
    _bar: String,
}

#[derive(FromQueryResult, DerivePartialModel)]
struct FieldFromExpr {
    #[pgorm(from_expr = "Column::Bar2.sum()")]
    _foo: f64,
    #[pgorm(from_expr = "Expr::col(Column::Id).equals(Column::Foo)")]
    _bar: bool,
}
