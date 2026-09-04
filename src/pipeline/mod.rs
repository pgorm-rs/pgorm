//! A PRQL-shaped pipeline query API, compiled through prqlc's PL AST.
//!
//! A [`Pipeline`] is a sequence of relation-to-relation transforms in the
//! shape of a [PRQL](https://prql-lang.org) query: instead of assembling
//! `SELECT` clauses, you append `filter`, `derive`, `aggregate_by`, `window`,
//! `sort` and `take` stages, and clause placement falls out of position — a
//! filter after an aggregation becomes `HAVING`, a filter after a window
//! wraps the pipeline in a CTE. Because every stage maps relation to
//! relation, any `fn(Pipeline) -> Pipeline` is a reusable, composable query
//! scope.
//!
//! The pipeline lowers typed Rust construction directly into prqlc's PL AST
//! (no PRQL text round-trip), then through `pl_to_rq` and `rq_to_sql` with
//! the PostgreSQL dialect. Every prqlc import lives in the private `adapter`
//! module, and the dependency is pinned exact. The pipeline is a permanent
//! part of the crate: prqlc is a plain dependency, compiled in every build.
//!
//! ```
//! use pgorm::pipeline::{Pipeline, col, count_rows, sum};
//! use pgorm::tests_cfg::{cake, fruit};
//!
//! let (sql, values) = Pipeline::from(cake::Entity)
//!     .join(
//!         pgorm::pipeline::JoinSide::Left,
//!         fruit::Entity,
//!         |_| {
//!             col(cake::Entity, cake::Column::Id).eq(col(fruit::Entity, fruit::Column::CakeId))
//!         },
//!     )
//!     .aggregate_by(|_| {
//!         (
//!             vec![col(cake::Entity, cake::Column::Name)],
//!             vec![count_rows().aliased("fruit_count")],
//!         )
//!     })
//!     .filter(|binder| pgorm::pipeline::out("fruit_count").gt(binder.bind(1_i64)))
//!     .sort(|_| vec![pgorm::pipeline::out("fruit_count").desc()])
//!     .take(10)
//!     .into_sql()
//!     .expect("the pipeline resolves");
//!
//! assert!(sql.contains("HAVING"));
//! assert_eq!(values.0.len(), 1);
//! # let _ = sum(col(cake::Entity, cake::Column::Id));
//! ```
//!
//! Runtime values enter through the [`Binder`] each transform closure
//! receives: [`Binder::bind`] mints the `$N` placeholder and records the
//! value in one step, and the returned expression is branded so it cannot
//! leak into a different pipeline. Everything fallible — reserved-alias
//! screening, prqlc's resolution — surfaces as a typed [`PipelineError`]
//! from [`Pipeline::into_sql`] or the terminal methods; nothing panics.
// [spec:pgorm:def:pipeline.adapter+2]

mod adapter;
mod binder;
mod builder;
mod error;
mod expr;
mod funcs;
mod terminal;

pub use binder::Binder;
pub use builder::{Frame, JoinSide, Pipeline, WindowDef};
pub use error::PipelineError;
pub use expr::{Expr, col, out};
pub use funcs::{
    CastType, average, case, count, count_distinct, count_rows, first, lag, last, lead, lit_bool,
    lit_float, lit_int, lit_str, max, min, null, rank, rank_dense, row_number, stddev, sum,
};

#[cfg(test)]
mod tests;
