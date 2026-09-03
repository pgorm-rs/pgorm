//! The pipeline itself: a source and a sequence of whole transforms.

use std::ops::RangeInclusive;

use pgorm_query::{Iden, Value};

use crate::EntityTrait;

use super::adapter::{self, PlExpr};
use super::binder::Binder;
use super::expr::Expr;

/// A relation-to-relation query pipeline in PRQL's shape.
///
/// [`from`](Pipeline::from) is the only way in, so a sourceless pipeline is
/// unrepresentable; every method appends one whole transform, so a
/// half-formed stage is unrepresentable too. Clause placement is the
/// compiler's job: a [`filter`](Pipeline::filter) lands in `WHERE`, `HAVING`
/// or a wrapping subquery according to where it sits in the pipeline, not
/// according to which method was called.
// [spec:pgorm:req:pipeline.surface]
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

/// A window frame for [`WindowDef::frame`], with 1-row / 1-value units
/// relative to the current row: `0` is the current row, negative is
/// preceding, positive is following, `None` leaves that side unbounded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    kind: FrameKind,
    start: Option<i64>,
    end: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FrameKind {
    Rows,
    Range,
}

impl Frame {
    /// A `ROWS BETWEEN ... AND ...` frame.
    pub fn rows(start: Option<i64>, end: Option<i64>) -> Self {
        Frame {
            kind: FrameKind::Rows,
            start,
            end,
        }
    }

    /// A `RANGE BETWEEN ... AND ...` frame.
    pub fn range(start: Option<i64>, end: Option<i64>) -> Self {
        Frame {
            kind: FrameKind::Range,
            start,
            end,
        }
    }

    fn named_arg(self) -> (&'static str, PlExpr) {
        let key = match self.kind {
            FrameKind::Rows => "rows",
            FrameKind::Range => "range",
        };
        (key, adapter::int_range(self.start, self.end))
    }
}

/// What a [`window`](Pipeline::window) stage computes: the derived columns,
/// and the partitioning, ordering and frame they are computed over.
#[derive(Debug)]
pub struct WindowDef<'brand> {
    pub(super) partition: Vec<Expr<'brand>>,
    pub(super) sort: Vec<Expr<'brand>>,
    pub(super) frame: Option<Frame>,
    pub(super) columns: Vec<Expr<'brand>>,
}

impl<'brand> WindowDef<'brand> {
    /// Start from the columns the window derives; partition, order and frame
    /// are added with the builder methods below.
    pub fn derive(columns: Vec<Expr<'brand>>) -> Self {
        WindowDef {
            partition: Vec::new(),
            sort: Vec::new(),
            frame: None,
            columns,
        }
    }

    /// `PARTITION BY` these expressions.
    pub fn partition_by(mut self, keys: Vec<Expr<'brand>>) -> Self {
        self.partition = keys;
        self
    }

    /// `ORDER BY` these keys within the window
    /// ([`desc`](Expr::desc) marks a key descending).
    ///
    /// Without a partition the sort is a real pipeline stage, so it also
    /// orders the output — PRQL semantics, kept rather than hidden.
    pub fn sorted(mut self, keys: Vec<Expr<'brand>>) -> Self {
        self.sort = keys;
        self
    }

    /// Restrict the frame the window functions see.
    pub fn frame(mut self, frame: Frame) -> Self {
        self.frame = Some(frame);
        self
    }
}

fn table_expr(table: impl Iden) -> PlExpr {
    adapter::ident(&Iden::to_string(&table))
}

impl Pipeline {
    /// Start a pipeline from a table.
    // [spec:pgorm:req:pipeline.surface]
    pub fn from(table: impl Iden) -> Self {
        Pipeline {
            stages: vec![adapter::call("from", vec![table_expr(table)])],
            values: Vec::new(),
        }
    }

    /// Start a pipeline from a schema-qualified table.
    // [spec:pgorm:sem:pipeline.qualify]
    pub fn from_schema(schema: impl Iden, table: impl Iden) -> Self {
        let source = adapter::ident_in(vec![Iden::to_string(&schema)], Iden::to_string(&table));
        Pipeline {
            stages: vec![adapter::call("from", vec![source])],
            values: Vec::new(),
        }
    }

    /// Start a pipeline from an entity's declared table, honouring its
    /// `schema_name` when it has one.
    // [spec:pgorm:sem:pipeline.qualify]
    pub fn from_entity<E: EntityTrait>() -> Self {
        let entity = E::default();
        match entity.schema_name() {
            Some(schema) => {
                let source =
                    adapter::ident_in(vec![schema.to_owned()], entity.table_name().to_owned());
                Pipeline {
                    stages: vec![adapter::call("from", vec![source])],
                    values: Vec::new(),
                }
            }
            None => Pipeline::from(entity),
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

    /// Keep rows the condition holds for.
    ///
    /// Placement follows position: before an [`aggregate_by`](Self::aggregate_by)
    /// this becomes `WHERE`, directly after one it becomes `HAVING`, and after
    /// a [`window`](Self::window) the pipeline so far is wrapped in a CTE and
    /// filtered outside it.
    pub fn filter<F>(mut self, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> Expr<'brand>,
    {
        let node = {
            let condition = f(&mut Binder::new(&mut self.values));
            adapter::call("filter", vec![condition.node])
        };
        self.stage(node)
    }

    /// Add computed columns, keeping the existing ones.
    pub fn derive<F>(mut self, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> Vec<Expr<'brand>>,
    {
        let node = {
            let columns = f(&mut Binder::new(&mut self.values));
            adapter::call("derive", vec![exprs_tuple(columns)])
        };
        self.stage(node)
    }

    /// Replace the projection with exactly these columns.
    pub fn select<F>(mut self, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> Vec<Expr<'brand>>,
    {
        let node = {
            let columns = f(&mut Binder::new(&mut self.values));
            adapter::call("select", vec![exprs_tuple(columns)])
        };
        self.stage(node)
    }

    /// Group by keys and aggregate: the closure returns
    /// `(keys, aggregates)`, and the resulting relation carries the keys
    /// followed by the aggregates.
    pub fn aggregate_by<F>(mut self, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> (Vec<Expr<'brand>>, Vec<Expr<'brand>>),
    {
        let node = {
            let (keys, aggregates) = f(&mut Binder::new(&mut self.values));
            adapter::call(
                "group",
                vec![
                    exprs_tuple(keys),
                    adapter::call("aggregate", vec![exprs_tuple(aggregates)]),
                ],
            )
        };
        self.stage(node)
    }

    /// Derive columns over a window; see [`WindowDef`].
    ///
    /// With a partition this compiles to `PARTITION BY` under a `group`
    /// stage; without one the window spans the whole relation.
    pub fn window<F>(mut self, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> WindowDef<'brand>,
    {
        let nodes = {
            let def = f(&mut Binder::new(&mut self.values));
            let derive_call = adapter::call("derive", vec![exprs_tuple(def.columns)]);
            let window_call = match def.frame {
                Some(frame) => {
                    adapter::call_named("window", vec![derive_call], vec![frame.named_arg()])
                }
                None => adapter::call("window", vec![derive_call]),
            };
            let sort_call = if def.sort.is_empty() {
                None
            } else {
                Some(adapter::call("sort", vec![exprs_tuple(def.sort)]))
            };
            if def.partition.is_empty() {
                sort_call.into_iter().chain([window_call]).collect()
            } else {
                let body = match sort_call {
                    Some(sort_call) => adapter::nested(vec![sort_call, window_call]),
                    None => window_call,
                };
                vec![adapter::call(
                    "group",
                    vec![exprs_tuple(def.partition), body],
                )]
            }
        };
        self.staged(nodes)
    }

    /// Sort by these keys ([`desc`](Expr::desc) marks a key descending).
    pub fn sort<F>(mut self, f: F) -> Self
    where
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> Vec<Expr<'brand>>,
    {
        let node = {
            let keys = f(&mut Binder::new(&mut self.values));
            adapter::call("sort", vec![exprs_tuple(keys)])
        };
        self.stage(node)
    }

    /// Keep the first `rows` rows (`LIMIT`).
    ///
    /// The count is a value, not an expression: PRQL rejects a parameterized
    /// `take`, so the signature takes the only form that compiles.
    // [spec:pgorm:req:pipeline.params]
    pub fn take(self, rows: i64) -> Self {
        self.stage(adapter::call("take", vec![adapter::lit_int(rows)]))
    }

    /// Keep an inclusive 1-based row range (`LIMIT`/`OFFSET`).
    pub fn take_range(self, rows: RangeInclusive<i64>) -> Self {
        self.stage(adapter::call(
            "take",
            vec![adapter::int_range(Some(*rows.start()), Some(*rows.end()))],
        ))
    }

    /// Join another table on an explicit condition.
    ///
    /// The condition names its columns through [`col`](super::col) on both
    /// sides, so it is qualified by construction.
    pub fn join<T, F>(mut self, side: JoinSide, table: T, on: F) -> Self
    where
        T: Iden,
        F: for<'brand> FnOnce(&mut Binder<'brand>) -> Expr<'brand>,
    {
        let node = {
            let condition = on(&mut Binder::new(&mut self.values));
            adapter::call_named(
                "join",
                vec![table_expr(table), condition.node],
                vec![("side", adapter::ident(side.keyword()))],
            )
        };
        self.stage(node)
    }
}

fn exprs_tuple(items: Vec<Expr<'_>>) -> PlExpr {
    adapter::tuple(items.into_iter().map(|item| item.node).collect())
}
