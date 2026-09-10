//! Owned expression instructions lowered only inside their Rust binder brand.

use std::sync::Arc;

use pgorm::pgorm_query::Alias;
use pgorm::pipeline::{self as pl, ExprOps};

#[derive(Clone, Copy, Debug)]
pub(super) enum Binary {
    Eq,
    Ne,
    Gt,
    Gte,
    Lt,
    Lte,
    And,
    Or,
    Coalesce,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
}

impl Binary {
    fn lower<'brand>(self, left: pl::Expr<'brand>, right: pl::Expr<'brand>) -> pl::Expr<'brand> {
        match self {
            Self::Eq => left.eq(right),
            Self::Ne => left.ne(right),
            Self::Gt => left.gt(right),
            Self::Gte => left.gte(right),
            Self::Lt => left.lt(right),
            Self::Lte => left.lte(right),
            Self::And => left.and(right),
            Self::Or => left.or(right),
            Self::Coalesce => left.coalesce(right),
            Self::Add => left.add(right),
            Self::Sub => left.sub(right),
            Self::Mul => left.mul(right),
            Self::Div => left.div(right),
            Self::Rem => left.rem(right),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Unary {
    Not,
    Neg,
    Asc,
    Desc,
    IsNull,
    IsNotNull,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum Function {
    Sum,
    Min,
    Max,
    Average,
    Stddev,
    Count,
    CountDistinct,
    Rank,
    RankDense,
    First,
    Last,
    Lag(i64),
    Lead(i64),
}

#[derive(Clone, Debug)]
pub(super) enum Recipe {
    Null,
    Bool(bool),
    Integer(i64),
    Float(f64),
    Text(String),
    Column(String, String),
    Alias(String),
    This(String),
    That(String),
    Bound(usize),
    Binary(Binary, Arc<Self>, Arc<Self>),
    Unary(Unary, Arc<Self>),
    Named(Arc<Self>, String),
    Cast(Arc<Self>, pl::CastType),
    In(Arc<Self>, Vec<Arc<Self>>),
    CountRows,
    RowNumber,
    Function(Function, Arc<Self>),
    Case(Vec<(Arc<Self>, Arc<Self>)>, Arc<Self>),
}

impl Recipe {
    // [spec:pgorm:req:python.pipeline]
    pub(super) fn lower<'brand>(&self, bound: &[pl::Expr<'brand>]) -> pl::Expr<'brand> {
        match self {
            Self::Null => pl::null(),
            Self::Bool(value) => (*value).into(),
            Self::Integer(value) => (*value).into(),
            Self::Float(value) => (*value).into(),
            Self::Text(value) => value.as_str().into(),
            Self::Column(table, column) => pl::col(Alias::new(table), Alias::new(column)),
            Self::Alias(name) => Alias::new(name).into(),
            Self::This(name) => pl::this(Alias::new(name)),
            Self::That(name) => pl::that(Alias::new(name)),
            // Only Scope::bind creates these indices; callbacks are validated before lowering.
            Self::Bound(index) => bound[*index].clone(),
            Self::Binary(operation, left, right) => {
                operation.lower(left.lower(bound), right.lower(bound))
            }
            Self::Unary(operation, value) => {
                let value = value.lower(bound);
                match operation {
                    Unary::Not => !value,
                    Unary::Neg => -value,
                    Unary::Asc => value.asc(),
                    Unary::Desc => value.desc(),
                    Unary::IsNull => value.is_null(),
                    Unary::IsNotNull => value.is_not_null(),
                }
            }
            Self::Named(value, name) => value.lower(bound).as_runtime(Alias::new(name)),
            Self::Cast(value, kind) => value.lower(bound).cast(*kind),
            Self::In(value, members) => value
                .lower(bound)
                .in_array(members.iter().map(|m| m.lower(bound))),
            Self::CountRows => pl::count_rows(),
            Self::RowNumber => pl::row_number(),
            Self::Function(function, argument) => {
                let argument = argument.lower(bound);
                match function {
                    Function::Sum => pl::sum(argument),
                    Function::Min => pl::min(argument),
                    Function::Max => pl::max(argument),
                    Function::Average => pl::average(argument),
                    Function::Stddev => pl::stddev(argument),
                    Function::Count => pl::count(argument),
                    Function::CountDistinct => pl::count_distinct(argument),
                    Function::Rank => pl::rank(argument),
                    Function::RankDense => pl::rank_dense(argument),
                    Function::First => pl::first(argument),
                    Function::Last => pl::last(argument),
                    Function::Lag(offset) => pl::lag(*offset, argument),
                    Function::Lead(offset) => pl::lead(*offset, argument),
                }
            }
            Self::Case(arms, otherwise) => pl::case(
                arms.iter()
                    .map(|(when, then)| (when.lower(bound), then.lower(bound))),
                otherwise.lower(bound),
            ),
        }
    }
}
