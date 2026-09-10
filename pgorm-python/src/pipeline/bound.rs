use pgorm::pipeline as pl;
use pyo3::prelude::*;

use super::scope::BoundPlan;
use crate::UnsupportedCapabilityError;

pub(super) enum Stage {
    Derive,
    Select,
    Sort,
    Window(pl::Over),
}

// Rust's list callbacks use const arity. Dispatch a declared Python bound-list range.
macro_rules! arity {
    ($plan:ident, $call:ident, $arguments:tt; $($size:literal),+) => {
        match $plan.nodes.len() {
            $($size => Ok(arity!(@call $call, $size, $arguments, $plan)),)+
            _ => Err(UnsupportedCapabilityError::new_err("bound pipeline lists support at most 32 expressions")),
        }
    };
    (@call $call:ident, $size:literal, ($($argument:expr),*), $plan:ident) => {
        $call::<$size>($($argument,)* $plan)
    };
}

fn list<const N: usize>(pipeline: pl::Pipeline, stage: Stage, plan: BoundPlan) -> pl::Pipeline {
    match stage {
        Stage::Derive => pipeline.derive_with(|binder| plan.lower::<N>(binder)),
        Stage::Select => pipeline.select_with(|binder| plan.lower::<N>(binder)),
        Stage::Sort => pipeline.sort_with(|binder| plan.lower::<N>(binder)),
        Stage::Window(over) => pipeline.window_with(over, |binder| plan.lower::<N>(binder)),
    }
}

pub(super) fn apply(
    pipeline: pl::Pipeline,
    stage: Stage,
    plan: BoundPlan,
) -> PyResult<pl::Pipeline> {
    arity!(plan, list, (pipeline, stage); 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32)
}

fn group<const N: usize>(pipeline: pl::Pipeline, plan: BoundPlan) -> pl::Grouped {
    pipeline.group_with(|binder| plan.lower::<N>(binder))
}

pub(super) fn grouped(pipeline: pl::Pipeline, plan: BoundPlan) -> PyResult<pl::Grouped> {
    arity!(plan, group, (pipeline); 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32)
}

fn aggregates<const N: usize>(group: pl::Grouped, plan: BoundPlan) -> pl::Pipeline {
    group.aggregate_with(|binder| plan.lower::<N>(binder))
}

pub(super) fn aggregate(group: pl::Grouped, plan: BoundPlan) -> PyResult<pl::Pipeline> {
    arity!(plan, aggregates, (group); 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16,
        17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32)
}
