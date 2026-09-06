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
// [spec:pgorm:req:pipeline.surface+3]
#[derive(Debug, Clone)]
pub struct Pipeline {
    pub(super) bindings: Vec<Vec<PlExpr>>,
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

/// A relation a pipeline can read: the argument of [`from`](Pipeline::from),
/// [`join`](Pipeline::join) and the set operations.
///
/// An entity brings its own table name and schema, so it is the ordinary
/// source; [`alias`](pgorm_query::alias) and [`Alias`] name a table no
/// entity describes; and a whole [`Pipeline`] is a relation too, embedded as
/// a `let`-bound subrelation.
// [spec:pgorm:sem:pipeline.qualify+2]
pub trait IntoSource {
    /// The relation, ready to embed.
    fn into_source(self) -> Source;

    /// Read this relation under a name of your own, so a pipeline can meet
    /// the same table twice.
    ///
    /// The name is the relation's only name from then on, as in SQL: an
    /// aliased entity no longer answers to its table name, and every
    /// reference to its columns goes through [`col`](super::col).
    ///
    /// ```
    /// # use pgorm::pipeline::{ExprOps, IntoSource, JoinSide, Pipeline, alias, col};
    /// # use pgorm::tests_cfg::fruit::{self, Column as F};
    /// let peer = alias("peer");
    /// let (sql, _) = Pipeline::from(fruit::Entity)
    ///     .join(
    ///         JoinSide::Inner,
    ///         fruit::Entity.named(peer),
    ///         F::CakeId.eq(col(peer, alias("cake_id"))),
    ///     )
    ///     .select((F::Name, col(peer, alias("name"))))
    ///     .into_sql()?;
    /// assert_eq!(
    ///     sql,
    ///     "SELECT fruit.name AS _expr_0, peer.name FROM fruit \
    ///      INNER JOIN fruit AS peer ON fruit.cake_id = peer.cake_id"
    /// );
    /// # Ok::<_, pgorm::pipeline::PipelineError>(())
    /// ```
    // [spec:pgorm:sem:pipeline.self-join]
    fn named(self, name: impl Into<AliasName>) -> Source
    where
        Self: Sized,
    {
        let mut source = self.into_source();
        source.alias = Some(name.into().as_str().to_owned());
        source
    }
}

/// A relation on its way into a pipeline, made by [`IntoSource`].
///
/// A table travels as its identifier; a pipeline travels whole — stages,
/// bindings and bound values — and is `let`-bound by the consumer. The
/// contents are not constructible outside the pipeline module, so the set of
/// relation shapes is closed.
// [spec:pgorm:req:pipeline.compose]
#[derive(Debug)]
pub struct Source {
    kind: SourceKind,
    alias: Option<String>,
}

#[derive(Debug)]
enum SourceKind {
    Table(PlExpr),
    Pipeline(Pipeline),
}

fn table_source(node: PlExpr) -> Source {
    Source {
        kind: SourceKind::Table(node),
        alias: None,
    }
}

/// A relation already carried as a [`Source`] — what
/// [`named`](IntoSource::named) returns — passes through unchanged.
// [spec:pgorm:sem:pipeline.self-join]
impl IntoSource for Source {
    fn into_source(self) -> Source {
        self
    }
}

// [spec:pgorm:sem:pipeline.qualify+2]
impl<E: EntityTrait> IntoSource for E {
    fn into_source(self) -> Source {
        table_source(match self.schema_name() {
            Some(schema) => {
                adapter::ident_in(vec![schema.to_owned()], self.table_name().to_owned())
            }
            None => adapter::ident(self.table_name()),
        })
    }
}

// [spec:pgorm:sem:pipeline.qualify+2]
impl IntoSource for AliasName {
    fn into_source(self) -> Source {
        table_source(adapter::ident(self.as_str()))
    }
}

// [spec:pgorm:sem:pipeline.qualify+2]
impl IntoSource for Alias {
    fn into_source(self) -> Source {
        table_source(adapter::ident(&Iden::to_string(&self)))
    }
}

/// A whole pipeline is a relation. Embedding consumes it by value, so its
/// bound values travel with its placeholders and the pair stays aligned; an
/// expression cannot make the same crossing alone
/// (`[spec:pgorm:req:pipeline.params+3]`).
// [spec:pgorm:req:pipeline.compose]
impl IntoSource for Pipeline {
    fn into_source(self) -> Source {
        Source {
            kind: SourceKind::Pipeline(self),
            alias: None,
        }
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
    // [spec:pgorm:req:pipeline.params+3]
    pub fn by(mut self, keys: impl ExprList<'static>) -> Self {
        self.partition = nodes_of(keys);
        self
    }

    /// `ORDER BY` these keys within the window.
    ///
    /// Without a partition the sort is a real pipeline stage, so it also
    /// orders the output — PRQL semantics, kept rather than hidden.
    // [spec:pgorm:req:pipeline.params+3]
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
// [spec:pgorm:req:pipeline.surface+3]
#[derive(Debug, Clone)]
pub struct Grouped {
    pipeline: Pipeline,
    keys: Vec<PlExpr>,
}

impl Grouped {
    /// Aggregate each group; the resulting relation carries the keys
    /// followed by the aggregates.
    // [spec:pgorm:req:pipeline.surface+3]
    pub fn aggregate(self, aggregates: impl ExprList<'static>) -> Pipeline {
        let nodes = nodes_of(aggregates);
        self.finish(nodes)
    }

    /// Aggregate each group, with runtime values bound in the closure.
    // [spec:pgorm:req:pipeline.params+3]
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
    /// Start a pipeline from a relation: an entity (schema and all), a table
    /// named some other way, or another pipeline embedded whole.
    // [spec:pgorm:req:pipeline.surface+3]
    pub fn from(source: impl IntoSource) -> Self {
        let mut pipeline = Pipeline {
            bindings: Vec::new(),
            stages: Vec::new(),
            values: Vec::new(),
        };
        let reference = pipeline.embed(source.into_source());
        pipeline.stages.push(adapter::call("from", vec![reference]));
        pipeline
    }

    /// Start a pipeline from a schema-qualified table no entity describes.
    // [spec:pgorm:sem:pipeline.qualify+2]
    pub fn from_schema(schema: impl Iden, table: impl Iden) -> Self {
        let source = adapter::ident_in(vec![Iden::to_string(&schema)], Iden::to_string(&table));
        Pipeline {
            bindings: Vec::new(),
            stages: vec![adapter::call("from", vec![source])],
            values: Vec::new(),
        }
    }

    /// Merge an embedded relation into this pipeline; the returned expression
    /// is how the stages refer to it.
    ///
    /// A table is its identifier. A pipeline becomes the next `let` binding:
    /// its values append to this pipeline's and its `$N` placeholders shift
    /// by the count already bound here, so position `N` in the merged SQL is
    /// still position `N` in the merged values; its own bindings come along,
    /// renumbered past the ones already present, with every internal
    /// reference rewritten to match.
    ///
    /// A [`named`](IntoSource::named) relation carries its alias onto the
    /// reference, which is what lets the same table appear twice.
    // [spec:pgorm:req:pipeline.compose]
    // [spec:pgorm:sem:pipeline.self-join]
    fn embed(&mut self, source: Source) -> PlExpr {
        let Source { kind, alias } = source;
        let reference = self.embed_kind(kind);
        match alias {
            Some(name) => adapter::aliased(reference, name),
            None => reference,
        }
    }

    fn embed_kind(&mut self, kind: SourceKind) -> PlExpr {
        match kind {
            SourceKind::Table(node) => node,
            SourceKind::Pipeline(other) => {
                let params = self.values.len();
                let binding_count = other.bindings.len();
                let binding_offset = self.bindings.len();
                self.values.extend(other.values);
                for mut binding in other.bindings {
                    for node in &mut binding {
                        adapter::rebase(node, params, binding_count, binding_offset);
                    }
                    self.bindings.push(binding);
                }
                let mut stages = other.stages;
                for node in &mut stages {
                    adapter::rebase(node, params, binding_count, binding_offset);
                }
                let name = adapter::binding_name(self.bindings.len());
                self.bindings.push(stages);
                adapter::ident(&name)
            }
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
    // [spec:pgorm:req:pipeline.surface+3]
    pub fn filter(self, condition: impl Into<Expr<'static>>) -> Self {
        self.stage(adapter::call("filter", vec![condition.into().node]))
    }

    /// Keep rows the condition holds for, with runtime values bound in the
    /// closure.
    // [spec:pgorm:req:pipeline.params+3]
    pub fn filter_with<F>(mut self, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> Expr<'brand>,
    {
        let node = self.bound(|binder| f(binder).node);
        self.stage(adapter::call("filter", vec![node]))
    }

    /// Add computed columns, keeping the existing ones.
    // [spec:pgorm:req:pipeline.surface+3]
    pub fn derive(self, columns: impl ExprList<'static>) -> Self {
        self.stage(adapter::call(
            "derive",
            vec![adapter::tuple(nodes_of(columns))],
        ))
    }

    /// Add computed columns, with runtime values bound in the closure.
    // [spec:pgorm:req:pipeline.params+3]
    pub fn derive_with<F, const N: usize>(mut self, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> [Expr<'brand>; N],
    {
        let nodes = self.bound_nodes(f);
        self.stage(adapter::call("derive", vec![adapter::tuple(nodes)]))
    }

    /// Replace the projection with exactly these columns.
    // [spec:pgorm:req:pipeline.surface+3]
    pub fn select(self, columns: impl ExprList<'static>) -> Self {
        self.stage(adapter::call(
            "select",
            vec![adapter::tuple(nodes_of(columns))],
        ))
    }

    /// Replace the projection, with runtime values bound in the closure.
    // [spec:pgorm:req:pipeline.params+3]
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
    // [spec:pgorm:req:pipeline.surface+3]
    pub fn group(self, keys: impl ExprList<'static>) -> Grouped {
        let keys = nodes_of(keys);
        Grouped {
            pipeline: self,
            keys,
        }
    }

    /// Group rows by keys computed with runtime values bound in the closure.
    // [spec:pgorm:req:pipeline.params+3]
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
    // [spec:pgorm:req:pipeline.surface+3]
    pub fn window(self, columns: impl ExprList<'static>, over: Over) -> Self {
        let derive_call = adapter::call("derive", vec![adapter::tuple(nodes_of(columns))]);
        self.staged(over.wrap(derive_call))
    }

    /// Derive columns over a window, with runtime values bound in the
    /// closure.
    ///
    /// The window spec comes first here so that the closure stays last, as
    /// it does in every `_with` transform.
    // [spec:pgorm:req:pipeline.params+3]
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
    // [spec:pgorm:req:pipeline.surface+3]
    pub fn sort(self, keys: impl ExprList<'static>) -> Self {
        self.stage(adapter::call("sort", vec![adapter::tuple(nodes_of(keys))]))
    }

    /// Sort by keys computed with runtime values bound in the closure.
    // [spec:pgorm:req:pipeline.params+3]
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
    // [spec:pgorm:req:pipeline.params+3]
    pub fn take(self, rows: i64) -> Self {
        self.stage(adapter::call("take", vec![adapter::lit_int(rows)]))
    }

    /// Keep an inclusive 1-based row range (`LIMIT`/`OFFSET`).
    // [spec:pgorm:req:pipeline.params+3]
    pub fn take_range(self, rows: RangeInclusive<i64>) -> Self {
        self.stage(adapter::call(
            "take",
            vec![adapter::int_range(Some(*rows.start()), Some(*rows.end()))],
        ))
    }

    /// Join another relation on an explicit condition.
    ///
    /// Both sides of the condition are columns, and an entity column carries
    /// its table, so the condition is qualified by construction. When the
    /// joined relation is a pipeline, its columns are referred to by their
    /// own names — unqualified — or by [`that`](super::that) where a name
    /// exists on both sides. To join a relation to itself, give the operand
    /// a name of its own with [`named`](IntoSource::named).
    // [spec:pgorm:req:pipeline.surface+3]
    // [spec:pgorm:sem:pipeline.self-join]
    pub fn join(
        self,
        side: JoinSide,
        relation: impl IntoSource,
        on: impl Into<Expr<'static>>,
    ) -> Self {
        self.join_node(side, relation, on.into().node)
    }

    /// Join another relation, with runtime values bound in the closure.
    // [spec:pgorm:req:pipeline.params+3]
    pub fn join_with<F>(mut self, side: JoinSide, relation: impl IntoSource, on: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> Expr<'brand>,
    {
        let node = self.bound(|binder| on(binder).node);
        self.join_node(side, relation, node)
    }

    fn join_node(mut self, side: JoinSide, relation: impl IntoSource, condition: PlExpr) -> Self {
        let reference = self.embed(relation.into_source());
        self.stage(adapter::call_named(
            "join",
            vec![reference, condition],
            vec![("side", adapter::ident(side.keyword()))],
        ))
    }

    /// Concatenate another relation's rows after this one's: PRQL's
    /// `append`, SQL's `UNION ALL`.
    ///
    /// Columns correspond by position, and prqlc refuses the append at
    /// [`into_sql`](Pipeline::into_sql) when it can see the two column
    /// counts differ. Follow with [`distinct`](Pipeline::distinct) for
    /// `UNION DISTINCT`.
    // [spec:pgorm:req:pipeline.compose]
    pub fn append(self, other: impl IntoSource) -> Self {
        self.set_op("append", other)
    }

    /// Keep only rows that also appear in `other`: PRQL's `intersect`,
    /// SQL's `INTERSECT ALL`.
    ///
    /// The result is a renamed relation, as under [`remove`](Self::remove).
    // [spec:pgorm:req:pipeline.compose]
    pub fn intersect(self, other: impl IntoSource) -> Self {
        self.set_op("intersect", other)
    }

    /// Drop rows that appear in `other`: PRQL's `remove`, SQL's
    /// `EXCEPT ALL` — each row in `other` cancels one matching row here,
    /// not all of them.
    ///
    /// The result is a renamed relation: stages after it refer to columns
    /// by bare name (an alias token), because the source qualification —
    /// `col(INVOICE, ..)`, an entity column — no longer resolves. After
    /// [`append`](Self::append) the left side's naming survives.
    // [spec:pgorm:req:pipeline.compose]
    pub fn remove(self, other: impl IntoSource) -> Self {
        self.set_op("remove", other)
    }

    fn set_op(mut self, op: &str, other: impl IntoSource) -> Self {
        let reference = self.embed(other.into_source());
        self.stage(adapter::call(op, vec![reference]))
    }

    /// Keep one copy of each distinct row: PRQL's `group this (take 1)`,
    /// rendered `SELECT DISTINCT` — or folded into `UNION DISTINCT` when it
    /// directly follows [`append`](Pipeline::append).
    // [spec:pgorm:req:pipeline.compose]
    pub fn distinct(self) -> Self {
        self.stage(adapter::call(
            "group",
            vec![
                adapter::ident("this"),
                adapter::call("take", vec![adapter::lit_int(1)]),
            ],
        ))
    }

    fn bound_nodes<F, const N: usize>(&mut self, f: F) -> Vec<PlExpr>
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> [Expr<'brand>; N],
    {
        self.bound(|binder| f(binder).into_iter().map(|expr| expr.node).collect())
    }
}
