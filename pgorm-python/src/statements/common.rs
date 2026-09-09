use pgorm::pgorm_query::{
    BinOper, Condition, Expr, IntoCondition, Query, ReturningClause, SelectStatement, SimpleExpr,
    Values,
};
use pyo3::{
    prelude::*,
    types::{PyInt, PyTuple},
};

use crate::{
    errors::ConstructionError,
    expressions::{AliasedExpr, Compiled, PyCondition, require_expr},
};

pub(super) fn condition(value: &Bound<'_, PyAny>) -> PyResult<Condition> {
    if let Ok(condition) = value.extract::<PyRef<'_, PyCondition>>() {
        Ok(condition.inner.clone())
    } else {
        Ok(require_expr(value)?.inner.into_condition())
    }
}

pub(super) fn project(query: &mut SelectStatement, items: &Bound<'_, PyTuple>) -> PyResult<()> {
    for item in items.iter() {
        if let Ok(item) = item.extract::<PyRef<'_, AliasedExpr>>() {
            query.expr_as(item.expr.inner.clone(), item.alias.alias());
        } else {
            query.expr(require_expr(&item)?.inner);
        }
    }
    Ok(())
}

pub(super) fn unsigned(value: &Bound<'_, PyAny>) -> PyResult<u64> {
    if !value.is_exact_instance_of::<PyInt>() {
        return Err(ConstructionError::new_err(
            "limit/offset requires an integer",
        ));
    }
    let value: u64 = value
        .extract()
        .map_err(|_| ConstructionError::new_err("limit/offset must be non-negative and fit u64"))?;
    if value > i64::MAX as u64 {
        return Err(ConstructionError::new_err(
            "limit/offset exceeds PostgreSQL's signed bigint range",
        ));
    }
    Ok(value)
}

pub(super) fn compiled((sql, values): (String, Values)) -> PyResult<Compiled> {
    if values.0.len() > 65535 {
        return Err(ConstructionError::new_err(
            "statement exceeds PostgreSQL's 65535-parameter limit",
        ));
    }
    Ok(Compiled { sql, values })
}

pub(super) fn expressions(items: &Bound<'_, PyTuple>) -> PyResult<Vec<SimpleExpr>> {
    items
        .iter()
        .map(|item| require_expr(&item).map(|expr| expr.inner))
        .collect()
}

pub(super) fn returning(items: &Bound<'_, PyTuple>) -> PyResult<ReturningClause> {
    if items.is_empty() {
        return Ok(Query::returning().all());
    }
    let items = items
        .iter()
        .map(|item| {
            if let Ok(item) = item.extract::<PyRef<'_, AliasedExpr>>() {
                Ok(item
                    .expr
                    .inner
                    .clone()
                    .binary(BinOper::As, Expr::col(item.alias.alias())))
            } else {
                Ok(require_expr(&item)?.inner)
            }
        })
        .collect::<PyResult<Vec<_>>>()?;
    Ok(Query::returning().exprs(items))
}
