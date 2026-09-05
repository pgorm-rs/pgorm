//! The pipeline itself: a source and a sequence of whole transforms.

use std::ops::RangeInclusive;

use pgorm_query::{Alias, AliasName, Iden, Value};

use crate::EntityTrait;

use super::adapter::{self, PlExpr};
use super::binder::Binder;
use super::expr::{Expr, ExprList, nodes_of};

/// A relation-to-relation query pipeline in PRQL's shape.
///
/// [`from`](Pipeline::from) is the only way in, so a sourceless pipeline is
/// unrepresentable; every method appends one whole transform, so a
/// half-formed stage is unrepresentable too. Clause placement is the
/// compiler's job: a [`filter`](Pipeline::filter) lands in `WHERE`, `HAVING`
/// or a wrapping subquery according to where it sits in the pipeline, not
/// according to which method was called.
///
/// Each transform comes in two forms. The plain one takes expressions by
/// value, which is every query whose constants are written in the source:
///
/// ```
/// # use pgorm::pipeline::{ExprOps, Pipeline};
/// # use pgorm::tests_cfg::cake::{self, Column as C};
/// let (sql, values) = Pipeline::from(cake::Entity)
///     .filter(C::Id.gt(10))
///     .sort(C::Name)
///     .take(5)
///     .into_sql()?;
/// assert_eq!(sql, "SELECT * FROM cake WHERE id > 10 ORDER BY name LIMIT 5");
/// assert!(values.0.is_empty());
/// # Ok::<_, pgorm::pipeline::PipelineError>(())
/// ```
///
/// The `_with` one takes a closure and hands it the [`Binder`], which is the
/// only door a runtime value enters by:
///
/// ```
/// # use pgorm::pipeline::{ExprOps, Pipeline};
/// # use pgorm::tests_cfg::cake::{self, Column as C};
/// let (sql, values) = Pipeline::from(cake::Entity)
///     .filter_with(|binder| C::Id.gt(binder.bind(10_i32)))
///     .into_sql()?;
/// assert_eq!(sql, "SELECT * FROM cake WHERE id > $1");
/// assert_eq!(values.0.len(), 1);
/// # Ok::<_, pgorm::pipeline::PipelineError>(())
/// ```
// [spec:pgorm:req:pipeline.surface+1]
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub(super) stages: Vec<PlExpr>,
    pub(super) values: Vec<Value>,
}

/// Which rows a [`join`](Pipeline::join) keeps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JoinSide {
    /// `JOIN`
    Inner,
    /// `LEFT JOIN`
    Left,
    /// `RIGHT JOIN`
    Right,
    /// `FULL JOIN`
    Full,
}

impl JoinSide {
    fn keyword(self) -> &'static str {
        match self {
            JoinSide::Inner => "inner",
            JoinSide::Left => "left",
            JoinSide::Right => "right",
            JoinSide::Full => "full",
        }
    }
}

/// A relation a pipeline can read from.
///
/// An entity brings its own table name and schema, so it is the ordinary
/// source; [`alias`](pgorm_query::alias) and [`Alias`] name a table no
/// entity describes.
// [spec:pgorm:sem:pipeline.qualify+1]
pub trait IntoSource {
    /// The identifier this relation is read by.
    fn into_source(self) -> PlExpr;
}

// [spec:pgorm:sem:pipeline.qualify+1]
impl<E: EntityTrait> IntoSource for E {
    fn into_source(self) -> PlExpr {
        match self.schema_name() {
            Some(schema) => {
                adapter::ident_in(vec![schema.to_owned()], self.table_name().to_owned())
            }
            None => adapter::ident(self.table_name()),
        }
    }
}

// [spec:pgorm:sem:pipeline.qualify+1]
impl IntoSource for AliasName {
    fn into_source(self) -> PlExpr {
        adapter::ident(self.as_str())
    }
}

// [spec:pgorm:sem:pipeline.qualify+1]
impl IntoSource for Alias {
    fn into_source(self) -> PlExpr {
        adapter::ident(&Iden::to_string(&self))
    }
}

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
// [spec:pgorm:req:pipeline.surface+1]
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
    // [spec:pgorm:req:pipeline.params+1]
    pub fn by(mut self, keys: impl ExprList<'static>) -> Self {
        self.partition = nodes_of(keys);
        self
    }

    /// `ORDER BY` these keys within the window.
    ///
    /// Without a partition the sort is a real pipeline stage, so it also
    /// orders the output — PRQL semantics, kept rather than hidden.
    // [spec:pgorm:req:pipeline.params+1]
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

    fn wrap(self, derive_call: PlExpr) -> Vec<PlExpr> {
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

/// A pipeline that has been grouped and is waiting for its aggregates.
///
/// [`Pipeline::group`] cannot produce a pipeline on its own — PRQL's `group`
/// is a transform over a body, and a grouping with nothing aggregated is not
/// a relation — so the only way back to a [`Pipeline`] is
/// [`aggregate`](Grouped::aggregate).
// [spec:pgorm:req:pipeline.surface+1]
#[derive(Debug, Clone)]
pub struct Grouped {
    pipeline: Pipeline,
    keys: Vec<PlExpr>,
}

impl Grouped {
    /// Aggregate each group; the resulting relation carries the keys
    /// followed by the aggregates.
    // [spec:pgorm:req:pipeline.surface+1]
    pub fn aggregate(self, aggregates: impl ExprList<'static>) -> Pipeline {
        let nodes = nodes_of(aggregates);
        self.finish(nodes)
    }

    /// Aggregate each group, with runtime values bound in the closure.
    // [spec:pgorm:req:pipeline.params+1]
    pub fn aggregate_with<F, const N: usize>(mut self, f: F) -> Pipeline
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> [Expr<'brand>; N],
    {
        let nodes = {
            let aggregates = f(&mut Binder::new(&mut self.pipeline.values));
            aggregates.into_iter().map(|expr| expr.node).collect()
        };
        self.finish(nodes)
    }

    fn finish(self, aggregates: Vec<PlExpr>) -> Pipeline {
        let stage = adapter::call(
            "group",
            vec![
                adapter::tuple(self.keys),
                adapter::call("aggregate", vec![adapter::tuple(aggregates)]),
            ],
        );
        self.pipeline.stage(stage)
    }
}

impl Pipeline {
    /// Start a pipeline from a relation: an entity (schema and all), or a
    /// table named some other way.
    // [spec:pgorm:req:pipeline.surface+1]
    pub fn from(source: impl IntoSource) -> Self {
        Pipeline {
            stages: vec![adapter::call("from", vec![source.into_source()])],
            values: Vec::new(),
        }
    }

    /// Start a pipeline from a schema-qualified table no entity describes.
    // [spec:pgorm:sem:pipeline.qualify+1]
    pub fn from_schema(schema: impl Iden, table: impl Iden) -> Self {
        let source = adapter::ident_in(vec![Iden::to_string(&schema)], Iden::to_string(&table));
        Pipeline {
            stages: vec![adapter::call("from", vec![source])],
            values: Vec::new(),
        }
    }

    fn stage(mut self, node: PlExpr) -> Self {
        self.stages.push(node);
        self
    }

    fn staged(mut self, nodes: Vec<PlExpr>) -> Self {
        self.stages.extend(nodes);
        self
    }

    fn bound<F, T>(&mut self, f: F) -> T
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> T,
    {
        f(&mut Binder::new(&mut self.values))
    }

    /// Keep rows the condition holds for.
    ///
    /// Placement follows position: before an [`aggregate`](Grouped::aggregate)
    /// this becomes `WHERE`, directly after one it becomes `HAVING`, and after
    /// a [`window`](Self::window) the pipeline so far is wrapped in a CTE and
    /// filtered outside it.
    // [spec:pgorm:req:pipeline.surface+1]
    pub fn filter(self, condition: impl Into<Expr<'static>>) -> Self {
        self.stage(adapter::call("filter", vec![condition.into().node]))
    }

    /// Keep rows the condition holds for, with runtime values bound in the
    /// closure.
    // [spec:pgorm:req:pipeline.params+1]
    pub fn filter_with<F>(mut self, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> Expr<'brand>,
    {
        let node = self.bound(|binder| f(binder).node);
        self.stage(adapter::call("filter", vec![node]))
    }

    /// Add computed columns, keeping the existing ones.
    // [spec:pgorm:req:pipeline.surface+1]
    pub fn derive(self, columns: impl ExprList<'static>) -> Self {
        self.stage(adapter::call(
            "derive",
            vec![adapter::tuple(nodes_of(columns))],
        ))
    }

    /// Add computed columns, with runtime values bound in the closure.
    // [spec:pgorm:req:pipeline.params+1]
    pub fn derive_with<F, const N: usize>(mut self, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> [Expr<'brand>; N],
    {
        let nodes = self.bound_nodes(f);
        self.stage(adapter::call("derive", vec![adapter::tuple(nodes)]))
    }

    /// Replace the projection with exactly these columns.
    // [spec:pgorm:req:pipeline.surface+1]
    pub fn select(self, columns: impl ExprList<'static>) -> Self {
        self.stage(adapter::call(
            "select",
            vec![adapter::tuple(nodes_of(columns))],
        ))
    }

    /// Replace the projection, with runtime values bound in the closure.
    // [spec:pgorm:req:pipeline.params+1]
    pub fn select_with<F, const N: usize>(mut self, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> [Expr<'brand>; N],
    {
        let nodes = self.bound_nodes(f);
        self.stage(adapter::call("select", vec![adapter::tuple(nodes)]))
    }

    /// Group rows by these keys; the aggregates follow.
    ///
    /// ```
    /// # use pgorm::pipeline::{ExprOps, Pipeline, sum};
    /// # use pgorm::pgorm_query::alias;
    /// # use pgorm::tests_cfg::cake::{self, Column as C};
    /// let spent = alias("spent");
    /// let (sql, _) = Pipeline::from(cake::Entity)
    ///     .group(C::Name)
    ///     .aggregate(sum(C::Id).as_(spent))
    ///     .filter(spent.gt(2))
    ///     .into_sql()?;
    /// assert!(sql.contains("GROUP BY name"));
    /// assert!(sql.contains("HAVING"));
    /// # Ok::<_, pgorm::pipeline::PipelineError>(())
    /// ```
    // [spec:pgorm:req:pipeline.surface+1]
    pub fn group(self, keys: impl ExprList<'static>) -> Grouped {
        let keys = nodes_of(keys);
        Grouped {
            pipeline: self,
            keys,
        }
    }

    /// Group rows by keys computed with runtime values bound in the closure.
    // [spec:pgorm:req:pipeline.params+1]
    pub fn group_with<F, const N: usize>(mut self, f: F) -> Grouped
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> [Expr<'brand>; N],
    {
        let keys = self.bound_nodes(f);
        Grouped {
            pipeline: self,
            keys,
        }
    }

    /// Derive columns over a window: what to compute, and what to compute it
    /// over ([`by`], [`sort_by`], [`over`]).
    ///
    /// With a partition this compiles to `PARTITION BY` under a `group`
    /// stage; without one the window spans the whole relation.
    // [spec:pgorm:req:pipeline.surface+1]
    pub fn window(self, columns: impl ExprList<'static>, over: Over) -> Self {
        let derive_call = adapter::call("derive", vec![adapter::tuple(nodes_of(columns))]);
        self.staged(over.wrap(derive_call))
    }

    /// Derive columns over a window, with runtime values bound in the
    /// closure.
    ///
    /// The window spec comes first here so that the closure stays last, as
    /// it does in every `_with` transform.
    // [spec:pgorm:req:pipeline.params+1]
    pub fn window_with<F, const N: usize>(mut self, over: Over, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> [Expr<'brand>; N],
    {
        let nodes = self.bound_nodes(f);
        let derive_call = adapter::call("derive", vec![adapter::tuple(nodes)]);
        self.staged(over.wrap(derive_call))
    }

    /// Sort by these keys ([`desc`](super::ExprOps::desc) marks one
    /// descending).
    // [spec:pgorm:req:pipeline.surface+1]
    pub fn sort(self, keys: impl ExprList<'static>) -> Self {
        self.stage(adapter::call("sort", vec![adapter::tuple(nodes_of(keys))]))
    }

    /// Sort by keys computed with runtime values bound in the closure.
    // [spec:pgorm:req:pipeline.params+1]
    pub fn sort_with<F, const N: usize>(mut self, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> [Expr<'brand>; N],
    {
        let nodes = self.bound_nodes(f);
        self.stage(adapter::call("sort", vec![adapter::tuple(nodes)]))
    }

    /// Keep the first `rows` rows (`LIMIT`).
    ///
    /// The count is a value, not an expression: PRQL rejects a parameterized
    /// `take`, so the signature takes the only form that compiles.
    // [spec:pgorm:req:pipeline.params+1]
    pub fn take(self, rows: i64) -> Self {
        self.stage(adapter::call("take", vec![adapter::lit_int(rows)]))
    }

    /// Keep an inclusive 1-based row range (`LIMIT`/`OFFSET`).
    // [spec:pgorm:req:pipeline.params+1]
    pub fn take_range(self, rows: RangeInclusive<i64>) -> Self {
        self.stage(adapter::call(
            "take",
            vec![adapter::int_range(Some(*rows.start()), Some(*rows.end()))],
        ))
    }

    /// Join another relation on an explicit condition.
    ///
    /// Both sides of the condition are columns, and an entity column carries
    /// its table, so the condition is qualified by construction.
    // [spec:pgorm:req:pipeline.surface+1]
    pub fn join(
        self,
        side: JoinSide,
        table: impl IntoSource,
        on: impl Into<Expr<'static>>,
    ) -> Self {
        self.join_node(side, table, on.into().node)
    }

    /// Join another relation, with runtime values bound in the closure.
    // [spec:pgorm:req:pipeline.params+1]
    pub fn join_with<F>(mut self, side: JoinSide, table: impl IntoSource, on: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> Expr<'brand>,
    {
        let node = self.bound(|binder| on(binder).node);
        self.join_node(side, table, node)
    }

    fn join_node(self, side: JoinSide, table: impl IntoSource, condition: PlExpr) -> Self {
        self.stage(adapter::call_named(
            "join",
            vec![table.into_source(), condition],
            vec![("side", adapter::ident(side.keyword()))],
        ))
    }

    fn bound_nodes<F, const N: usize>(&mut self, f: F) -> Vec<PlExpr>
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> [Expr<'brand>; N],
    {
        self.bound(|binder| f(binder).into_iter().map(|expr| expr.node).collect())
    }
}
