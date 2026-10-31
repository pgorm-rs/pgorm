//! A stage whose expressions bind nothing takes them as pgorm's plain
//! transform does; one that binds — a value an operand was given, or a binder
//! function's placeholders — takes them through the transform's `_with` form,
//! lowering them inside the binder closure.

use pgorm::pipeline as pl;
use pyo3::prelude::*;

use super::scope::{Plan, unlowered};
use crate::errors::ConstructionError;

/// The most expressions a stage that binds takes: pgorm's `_with` closures
/// return a fixed-size array, which the binding dispatches up to this size.
const BOUND_LIST: usize = 32;

pub(super) enum Stage {
    Derive,
    Select,
    Sort,
    Window(pl::Over),
}

/// `$call::<N>(..)` for the plan's length `N`, up to the bound the binding
/// dispatches; `None` past it.
macro_rules! arity {
    ($len:expr, $call:ident, $arguments:tt) => {
        arity!(@sizes $len, $call, $arguments; 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14,
            15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32)
    };
    (@sizes $len:expr, $call:ident, $arguments:tt; $($size:literal),+) => {
        match $len {
            $($size => Some($call::<$size> $arguments),)+
            _ => None,
        }
    };
}

/// The stage's result: a list past the dispatched bound refused, and a
/// lowering that could not finish, which no input should cause, reported.
// [spec:pgorm:req:bindings.pipeline-binder]
fn finished<T>(result: Option<T>, failed: bool, count: usize) -> PyResult<T> {
    match result {
        None => Err(ConstructionError::new_err(format!(
            "a pipeline stage that binds takes at most {BOUND_LIST} expressions, not {count}"
        ))),
        Some(_) if failed => Err(unlowered()),
        Some(result) => Ok(result),
    }
}

fn list_n<const N: usize>(
    pipeline: pl::Pipeline,
    stage: Stage,
    plan: &Plan,
    failed: &mut bool,
) -> pl::Pipeline {
    match stage {
        Stage::Derive => pipeline.derive_with(|binder| plan.lower::<N>(binder, failed)),
        Stage::Select => pipeline.select_with(|binder| plan.lower::<N>(binder, failed)),
        Stage::Sort => pipeline.sort_with(|binder| plan.lower::<N>(binder, failed)),
        Stage::Window(over) => pipeline.window_with(over, |binder| plan.lower::<N>(binder, failed)),
    }
}

pub(super) fn apply(pipeline: pl::Pipeline, stage: Stage, plan: Plan) -> PyResult<pl::Pipeline> {
    if !plan.binds() {
        let plain = plan.unbound()?;
        return Ok(match stage {
            Stage::Derive => pipeline.derive(plain),
            Stage::Select => pipeline.select(plain),
            Stage::Sort => pipeline.sort(plain),
            Stage::Window(over) => pipeline.window(plain, over),
        });
    }
    let mut failed = false;
    let count = plan.nodes.len();
    let result = arity!(count, list_n, (pipeline, stage, &plan, &mut failed));
    finished(result, failed, count)
}

fn group_n<const N: usize>(pipeline: pl::Pipeline, plan: &Plan, failed: &mut bool) -> pl::Grouped {
    pipeline.group_with(|binder| plan.lower::<N>(binder, failed))
}

pub(super) fn grouped(pipeline: pl::Pipeline, plan: Plan) -> PyResult<pl::Grouped> {
    if !plan.binds() {
        return Ok(pipeline.group(plan.unbound()?));
    }
    let mut failed = false;
    let count = plan.nodes.len();
    let result = arity!(count, group_n, (pipeline, &plan, &mut failed));
    finished(result, failed, count)
}

fn aggregate_n<const N: usize>(group: pl::Grouped, plan: &Plan, failed: &mut bool) -> pl::Pipeline {
    group.aggregate_with(|binder| plan.lower::<N>(binder, failed))
}

pub(super) fn aggregate(group: pl::Grouped, plan: Plan) -> PyResult<pl::Pipeline> {
    if !plan.binds() {
        return Ok(group.aggregate(plan.unbound()?));
    }
    let mut failed = false;
    let count = plan.nodes.len();
    let result = arity!(count, aggregate_n, (group, &plan, &mut failed));
    finished(result, failed, count)
}

pub(super) fn filter(pipeline: pl::Pipeline, plan: Plan) -> PyResult<pl::Pipeline> {
    if !plan.binds() {
        let [predicate] = single(plan.unbound()?)?;
        return Ok(pipeline.filter(predicate));
    }
    let mut failed = false;
    let filtered = pipeline.filter_with(|binder| {
        let [predicate] = plan.lower::<1>(binder, &mut failed);
        predicate
    });
    finished(Some(filtered), failed, 1)
}

pub(super) fn join(
    pipeline: pl::Pipeline,
    side: pl::JoinSide,
    source: pl::Source,
    plan: Plan,
) -> PyResult<pl::Pipeline> {
    if !plan.binds() {
        let [on] = single(plan.unbound()?)?;
        return Ok(pipeline.join(side, source, on));
    }
    let mut failed = false;
    let joined = pipeline.join_with(side, source, |binder| {
        let [on] = plan.lower::<1>(binder, &mut failed);
        on
    });
    finished(Some(joined), failed, 1)
}

fn single(expressions: Vec<pl::Expr<'static>>) -> PyResult<[pl::Expr<'static>; 1]> {
    <[_; 1]>::try_from(expressions).map_err(|_| unlowered())
}
