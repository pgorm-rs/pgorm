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

/// A `$N` placeholder. `ExprKind::Param` passes through lowering verbatim,
/// so an index minted here that survives is the index the emitted SQL
/// carries — but the optimizer may prune the expression around it, which is
/// what the census in `into_sql` accounts for.
// [spec:pgorm:req:pipeline.params+3]
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

/// The internal name of the `index`-th `let` binding: `table_N`.
///
/// The same namespace prqlc mints its own wrapping CTEs in; its namer steps
/// around taken names, so the two sequences never collide.
// [spec:pgorm:req:pipeline.compose]
pub(super) fn binding_name(index: usize) -> String {
    format!("table_{index}")
}

fn binding_index(name: &str) -> Option<usize> {
    name.strip_prefix("table_")?.parse().ok()
}

/// Shift an embedded pipeline's frame so it keeps meaning inside its
/// consumer: every `$N` placeholder moves up by `params`, and every
/// reference to one of the pipeline's own `bindings` (of which it had
/// `binding_count`) moves up by `binding_offset`.
// [spec:pgorm:req:pipeline.compose]
pub(super) fn rebase(
    node: &mut PlExpr,
    params: usize,
    binding_count: usize,
    binding_offset: usize,
) {
    match &mut node.kind {
        ExprKind::Param(index) => {
            if let Ok(position) = index.parse::<usize>() {
                *index = (position + params).to_string();
            }
        }
        ExprKind::Ident(ident) => {
            if ident.path.is_empty()
                && let Some(index) = binding_index(&ident.name)
                && index < binding_count
            {
                ident.name = binding_name(index + binding_offset);
            }
        }
        ExprKind::Tuple(items) | ExprKind::Array(items) => {
            for item in items {
                rebase(item, params, binding_count, binding_offset);
            }
        }
        ExprKind::Pipeline(pipeline) => {
            for item in &mut pipeline.exprs {
                rebase(item, params, binding_count, binding_offset);
            }
        }
        ExprKind::Range(range) => {
            for bound in [&mut range.start, &mut range.end].into_iter().flatten() {
                rebase(bound, params, binding_count, binding_offset);
            }
        }
        ExprKind::Binary(node) => {
            rebase(&mut node.left, params, binding_count, binding_offset);
            rebase(&mut node.right, params, binding_count, binding_offset);
        }
        ExprKind::Unary(node) => rebase(&mut node.expr, params, binding_count, binding_offset),
        ExprKind::FuncCall(node) => {
            for arg in node.args.iter_mut().chain(node.named_args.values_mut()) {
                rebase(arg, params, binding_count, binding_offset);
            }
        }
        ExprKind::Case(arms) => {
            for arm in arms {
                rebase(&mut arm.condition, params, binding_count, binding_offset);
                rebase(&mut arm.value, params, binding_count, binding_offset);
            }
        }
        _ => {}
    }
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

/// Lower the assembled bindings and stages through prqlc: PL → RQ →
/// PostgreSQL SQL.
///
/// Each binding becomes a `let table_N = (...)` statement and the stages
/// become the `main` variable of an anonymous module — the same shape
/// `prqlc::prql_to_pl` produces for query text, so direct construction and
/// text compilation are interchangeable. prqlc lowers each referenced
/// binding to a CTE (or inlines it where SQL allows).
// [spec:pgorm:def:pipeline.adapter+2]
// [spec:pgorm:req:pipeline.compose]
pub(super) fn compile(bindings: Vec<Vec<PlExpr>>, stages: Vec<PlExpr>) -> Result<String, String> {
    let stmts = bindings
        .into_iter()
        .enumerate()
        .map(|(index, stages)| stmt(VarDefKind::Let, binding_name(index), stages))
        .chain([stmt(VarDefKind::Main, "main".into(), stages)])
        .collect();
    let module = ModuleDef {
        name: "Project".into(),
        stmts,
    };
    let options = Options::default()
        .with_target(Target::Sql(Some(Dialect::Postgres)))
        .no_format()
        .no_signature();
    prqlc::pl_to_rq(module)
        .and_then(|rq| prqlc::rq_to_sql(rq, &options))
        .map_err(|errors| errors.to_string())
}

fn stmt(kind: VarDefKind, name: String, stages: Vec<PlExpr>) -> Stmt {
    Stmt {
        kind: StmtKind::VarDef(VarDef {
            kind,
            name,
            value: Some(Box::new(nested(stages))),
            ty: None,
        }),
        span: None,
        annotations: vec![],
        doc_comment: None,
    }
}

#[cfg(test)]
pub(super) fn compile_text(prql: &str) -> Result<String, String> {
    let options = Options::default()
        .with_target(Target::Sql(Some(Dialect::Postgres)))
        .no_format()
        .no_signature();
    prqlc::compile(prql, &options).map_err(|errors| errors.to_string())
}
