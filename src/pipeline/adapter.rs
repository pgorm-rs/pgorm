//! The one seam between the pipeline builder and prqlc.
//!
//! Every `prqlc` import in pgorm lives in this file. The PL AST is not a
//! stable API, so the dependency is pinned exact (`=0.13.14`) and the rest of
//! the module works against the constructors below rather than the AST types;
//! a compiler bump is absorbed by rewriting this file alone.

use prqlc::pr::{
    BinaryExpr, Expr, ExprKind, FuncCall, Ident, Literal, ModuleDef, Pipeline, Range, Stmt,
    StmtKind, SwitchCase, UnaryExpr, VarDef, VarDefKind,
};
use prqlc::sql::Dialect;
use prqlc::{Options, Target};

pub(super) use prqlc::pr::{BinOp, UnOp};

/// The PL expression node the builder assembles.
// [spec:pgorm:def:pipeline.adapter+2]
pub(super) type PlExpr = Expr;

pub(super) fn ident(name: &str) -> PlExpr {
    Expr::new(ExprKind::Ident(Ident::from_name(name)))
}

pub(super) fn ident_in(path: Vec<String>, name: String) -> PlExpr {
    Expr::new(ExprKind::Ident(Ident { path, name }))
}

pub(super) fn call(name: &str, args: Vec<PlExpr>) -> PlExpr {
    Expr::new(ExprKind::FuncCall(FuncCall {
        name: Box::new(ident(name)),
        args,
        named_args: Default::default(),
    }))
}

pub(super) fn call_named(name: &str, args: Vec<PlExpr>, named: Vec<(&str, PlExpr)>) -> PlExpr {
    Expr::new(ExprKind::FuncCall(FuncCall {
        name: Box::new(ident(name)),
        args,
        named_args: named.into_iter().map(|(k, v)| (k.into(), v)).collect(),
    }))
}

pub(super) fn tuple(items: Vec<PlExpr>) -> PlExpr {
    Expr::new(ExprKind::Tuple(items))
}

pub(super) fn array(items: Vec<PlExpr>) -> PlExpr {
    Expr::new(ExprKind::Array(items))
}

pub(super) fn nested(exprs: Vec<PlExpr>) -> PlExpr {
    Expr::new(ExprKind::Pipeline(Pipeline { exprs }))
}

pub(super) fn binary(left: PlExpr, op: BinOp, right: PlExpr) -> PlExpr {
    Expr::new(ExprKind::Binary(BinaryExpr {
        left: Box::new(left),
        op,
        right: Box::new(right),
    }))
}

pub(super) fn unary(op: UnOp, operand: PlExpr) -> PlExpr {
    Expr::new(ExprKind::Unary(UnaryExpr {
        op,
        expr: Box::new(operand),
    }))
}

pub(super) fn lit_int(value: i64) -> PlExpr {
    Expr::new(ExprKind::Literal(Literal::Integer(value)))
}

pub(super) fn lit_float(value: f64) -> PlExpr {
    Expr::new(ExprKind::Literal(Literal::Float(value)))
}

pub(super) fn lit_str(value: &str) -> PlExpr {
    Expr::new(ExprKind::Literal(Literal::String(value.to_owned())))
}

pub(super) fn lit_bool(value: bool) -> PlExpr {
    Expr::new(ExprKind::Literal(Literal::Boolean(value)))
}

pub(super) fn lit_null() -> PlExpr {
    Expr::new(ExprKind::Literal(Literal::Null))
}

/// A `$N` placeholder. `ExprKind::Param` survives lowering untouched, so the
/// index minted here is the index the emitted SQL carries.
// [spec:pgorm:req:pipeline.params]
pub(super) fn param(index: usize) -> PlExpr {
    Expr::new(ExprKind::Param(index.to_string()))
}

pub(super) fn aliased(mut node: PlExpr, alias: String) -> PlExpr {
    node.alias = Some(alias);
    node
}

pub(super) fn case(arms: Vec<(PlExpr, PlExpr)>) -> PlExpr {
    Expr::new(ExprKind::Case(
        arms.into_iter()
            .map(|(condition, value)| SwitchCase {
                condition: Box::new(condition),
                value: Box::new(value),
            })
            .collect(),
    ))
}

pub(super) fn int_range(start: Option<i64>, end: Option<i64>) -> PlExpr {
    Expr::new(ExprKind::Range(Range {
        start: start.map(|v| Box::new(lit_int(v))),
        end: end.map(|v| Box::new(lit_int(v))),
    }))
}

/// Every alias set anywhere in `node`, in construction order.
pub(super) fn collect_aliases(node: &PlExpr, found: &mut Vec<String>) {
    if let Some(alias) = &node.alias {
        found.push(alias.clone());
    }
    match &node.kind {
        ExprKind::Tuple(items) | ExprKind::Array(items) => {
            for item in items {
                collect_aliases(item, found);
            }
        }
        ExprKind::Pipeline(pipeline) => {
            for item in &pipeline.exprs {
                collect_aliases(item, found);
            }
        }
        ExprKind::Range(range) => {
            for bound in [&range.start, &range.end].into_iter().flatten() {
                collect_aliases(bound, found);
            }
        }
        ExprKind::Binary(node) => {
            collect_aliases(&node.left, found);
            collect_aliases(&node.right, found);
        }
        ExprKind::Unary(node) => collect_aliases(&node.expr, found),
        ExprKind::FuncCall(node) => {
            collect_aliases(&node.name, found);
            for arg in node.args.iter().chain(node.named_args.values()) {
                collect_aliases(arg, found);
            }
        }
        ExprKind::Case(arms) => {
            for arm in arms {
                collect_aliases(&arm.condition, found);
                collect_aliases(&arm.value, found);
            }
        }
        _ => {}
    }
}

/// Lower the assembled stages through prqlc: PL → RQ → PostgreSQL SQL.
///
/// The stages become the `main` variable of an anonymous module, the same
/// shape `prqlc::prql_to_pl` produces for query text, so direct construction
/// and text compilation are interchangeable.
// [spec:pgorm:def:pipeline.adapter+2]
pub(super) fn compile(stages: Vec<PlExpr>) -> Result<String, String> {
    let module = ModuleDef {
        name: "Project".into(),
        stmts: vec![Stmt {
            kind: StmtKind::VarDef(VarDef {
                kind: VarDefKind::Main,
                name: "main".into(),
                value: Some(Box::new(nested(stages))),
                ty: None,
            }),
            span: None,
            annotations: vec![],
            doc_comment: None,
        }],
    };
    let options = Options::default()
        .with_target(Target::Sql(Some(Dialect::Postgres)))
        .no_format()
        .no_signature();
    prqlc::pl_to_rq(module)
        .and_then(|rq| prqlc::rq_to_sql(rq, &options))
        .map_err(|errors| errors.to_string())
}

#[cfg(test)]
pub(super) fn compile_text(prql: &str) -> Result<String, String> {
    let options = Options::default()
        .with_target(Target::Sql(Some(Dialect::Postgres)))
        .no_format()
        .no_signature();
    prqlc::compile(prql, &options).map_err(|errors| errors.to_string())
}
