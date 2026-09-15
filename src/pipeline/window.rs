//! What a window computes its columns over: partitioning, ordering and frame.
//!
//! Separate from the pipeline builder because a window spec is built and
//! finished on its own, and reaches the pipeline only as a finished value.

use super::adapter::{self, PlExpr};
use super::expr::{ExprList, nodes_of};

/// What a [`window`](Pipeline::window) computes its columns over:
/// partitioning, ordering and frame.
///
/// Built by [`by`] (partition), [`sort_by`] (ordering) or [`over`] (neither),
/// then narrowed with [`rows`](Over::rows) or [`range`](Over::range).
///
/// The keys are `ExprList<'static>`, so a bound placeholder cannot enter a
/// window spec: `Over` erases the brand it was built from, and a partition
/// or ordering by a runtime value means nothing anyway.
///
/// ```compile_fail,E0521
/// use pgorm::pipeline::{ExprOps, Pipeline, by};
/// use pgorm::tests_cfg::cake::{self, Column as C};
///
/// let _ = Pipeline::from(cake::Entity).filter_with(|binder| {
///     let _smuggled = by(binder.bind(1_i32));
///     C::Id.gt(1)
/// });
/// ```
// [spec:pgorm:req:pipeline.surface+3]
#[derive(Debug, Clone, Default)]
pub struct Over {
    partition: Vec<PlExpr>,
    sort: Vec<PlExpr>,
    frame: Option<(&'static str, Option<i64>, Option<i64>)>,
}

/// A window over the whole relation, unpartitioned and unordered.
pub fn over() -> Over {
    Over::default()
}

/// A window partitioned by these keys: PRQL's `group`, SQL's `PARTITION BY`.
pub fn by(keys: impl ExprList<'static>) -> Over {
    over().by(keys)
}

/// A window ordered by these keys ([`desc`](super::ExprOps::desc) marks one
/// descending).
pub fn sort_by(keys: impl ExprList<'static>) -> Over {
    over().sort_by(keys)
}

impl Over {
    /// `PARTITION BY` these keys.
    // [spec:pgorm:req:pipeline.params+4]
    pub fn by(mut self, keys: impl ExprList<'static>) -> Self {
        self.partition = nodes_of(keys);
        self
    }

    /// `ORDER BY` these keys within the window.
    ///
    /// Without a partition the sort is a real pipeline stage, so it also
    /// orders the output — PRQL semantics, kept rather than hidden.
    // [spec:pgorm:req:pipeline.params+4]
    pub fn sort_by(mut self, keys: impl ExprList<'static>) -> Self {
        self.sort = nodes_of(keys);
        self
    }

    /// A `ROWS BETWEEN ... AND ...` frame, in rows relative to the current
    /// row: `0` is the current row, negative precedes, positive follows, and
    /// `None` leaves that side unbounded.
    pub fn rows(mut self, start: Option<i64>, end: Option<i64>) -> Self {
        self.frame = Some(("rows", start, end));
        self
    }

    /// A `RANGE BETWEEN ... AND ...` frame, in values, with bounds read as
    /// in [`rows`](Over::rows).
    pub fn range(mut self, start: Option<i64>, end: Option<i64>) -> Self {
        self.frame = Some(("range", start, end));
        self
    }

    pub(super) fn wrap(self, derive_call: PlExpr) -> Vec<PlExpr> {
        let window_call = match self.frame {
            Some((kind, start, end)) => adapter::call_named(
                "window",
                vec![derive_call],
                vec![(kind, adapter::int_range(start, end))],
            ),
            None => adapter::call("window", vec![derive_call]),
        };
        let sort_call = if self.sort.is_empty() {
            None
        } else {
            Some(adapter::call("sort", vec![adapter::tuple(self.sort)]))
        };
        if self.partition.is_empty() {
            sort_call.into_iter().chain([window_call]).collect()
        } else {
            let body = match sort_call {
                Some(sort_call) => adapter::nested(vec![sort_call, window_call]),
                None => window_call,
            };
            vec![adapter::call(
                "group",
                vec![adapter::tuple(self.partition), body],
            )]
        }
    }
}
