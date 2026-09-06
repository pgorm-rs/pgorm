use pgorm::entity::prelude::*;

#[derive(DerivePartialModel)]
#[pgorm(entity = "Entity")]
struct Partial {
    #[pgorm(from_col = "name", from_expr = "Expr::val(1)")]
    label: String,
}

fn main() {}
