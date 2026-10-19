#![allow(unused_imports, dead_code)]

//! A column's `DEFAULT` against a live server.
//!
//! A column definition reads its default as a `b_expr`, the grammar's narrow
//! operand with no logical operator, `IS`, `BETWEEN`, `IN` or `LIKE`, so an
//! operator expression there is parenthesised; `ALTER COLUMN .. SET DEFAULT`
//! reads the full expression and takes one bare. Each default here is read
//! back from a row written without the column, so a default the server
//! parsed as something else would show in the value.

pub mod common;
pub use common::{TestContext, setup::*};
use pgorm::pgorm_query::{ColumnDef, Expr, Name, SimpleExpr, Table};
use pgorm::{ConnectionTrait, Error};

fn n(name: &str) -> Name {
    Name::runtime(name)
}

fn both() -> SimpleExpr {
    SimpleExpr::from(Expr::val(true)).and(Expr::val(false).into())
}

// [spec:pgorm:req:sql.ddl.column-def+13/test]    against a live server: an operator expression
// is a column's DEFAULT in CREATE TABLE and ADD COLUMN, and SET DEFAULT takes one bare
#[pgorm_macros::test]
async fn operator_defaults_reach_the_server() -> Result<(), Error> {
    let ctx = TestContext::new("operator_defaults_reach_the_server").await;
    let db = ctx.db.get().await?;

    let create = Table::create(n("flag"))
        .col(ColumnDef::new(n("id")).integer())
        .col(ColumnDef::new(n("set")).boolean().default(both().not()))
        .col(
            ColumnDef::new(n("sum"))
                .integer()
                .default(SimpleExpr::from(Expr::val(1)).add(2).mul(3)),
        )
        .to_string();
    db.batch_execute(&create).await?;
    let add = Table::alter(n("flag"))
        .add_column(
            ColumnDef::new(n("between"))
                .boolean()
                .default(Expr::val(2).between(1, 3)),
        )
        .to_string();
    db.batch_execute(&add).await?;
    db.batch_execute("INSERT INTO flag (id) VALUES (1)").await?;

    let set = Table::alter(n("flag"))
        .modify_column(ColumnDef::new(n("set")).default(both()))
        .to_string();
    assert!(set.ends_with(r#"SET DEFAULT TRUE AND FALSE"#), "{set}");
    db.batch_execute(&set).await?;
    db.batch_execute("INSERT INTO flag (id) VALUES (2)").await?;

    let rows = db
        .query_all(
            r#"SELECT id, "set", "sum", "between" FROM flag ORDER BY id"#,
            &[],
        )
        .await?;
    let read: Vec<(i32, bool, i32, bool)> = rows
        .iter()
        .map(|row| (row.get(0), row.get(1), row.get(2), row.get(3)))
        .collect();
    assert_eq!(read, [(1, true, 9, true), (2, false, 9, true)]);

    drop(db);
    ctx.delete().await;
    Ok(())
}
