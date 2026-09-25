//! Grouping elements: the items of a `GROUP BY` list beyond a plain
//! expression — `()`, a parenthesised set, `ROLLUP`, `CUBE` and
//! `GROUPING SETS` — and the `SelectStatement` method that adds one.

use crate::{Func, SelectStatement, SimpleExpr};

/// One item of a `GROUP BY` list: PostgreSQL's `grouping_element`.
///
/// Each element stands for a list of grouping sets, and a `GROUP BY` list
/// groups by every combination of its elements' sets. A plain expression is
/// one set of one column, which is what [`SelectStatement::add_group_by`]
/// adds; the constructors here build the rest:
///
/// - [`set`](Self::set) — one set of several expressions, `("a", "b")`, and
///   [`empty`](Self::empty), the set of none, `()`, which groups the whole
///   input into one grand-total row;
/// - [`rollup`](Self::rollup) — `ROLLUP ("a", "b")`, the sets `("a", "b")`,
///   `("a")` and `()`: every prefix, for subtotals down a hierarchy;
/// - [`cube`](Self::cube) — `CUBE ("a", "b")`, every subset of the list;
/// - [`sets`](Self::sets) — `GROUPING SETS ( … )`, exactly the sets its
///   elements stand for, one after another.
///
/// Rows grouped by a set that leaves a column out carry NULL in it, and
/// [`Func::grouping`] is how a query tells that NULL from one in the data.
// [spec:pgorm:def:sql.ast.select.grouping]
#[derive(Debug, Clone, PartialEq)]
pub struct GroupingElement {
    pub(crate) kind: GroupingKind,
}

/// What a [`GroupingElement`] holds. Private, so the one list PostgreSQL
/// cannot take empty — `GROUPING SETS` — is non-empty by construction.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum GroupingKind {
    /// One set: a bare expression, a parenthesised list, or `()`.
    Set(Vec<SimpleExpr>),
    /// `ROLLUP ( … )`; with no expressions, the one set `()`.
    Rollup(Vec<SimpleExpr>),
    /// `CUBE ( … )`; with no expressions, the one set `()`.
    Cube(Vec<SimpleExpr>),
    /// `GROUPING SETS ( … )`; never empty.
    Sets(Vec<GroupingElement>),
}

/// A `GROUPING SETS ( … )` element under construction, as returned by
/// [`GroupingElement::sets`]: it holds its first element from the start, so
/// the empty list PostgreSQL rejects has no value to build.
// [spec:pgorm:def:sql.ast.select.grouping]
#[derive(Debug, Clone, PartialEq)]
pub struct GroupingSets {
    sets: Vec<GroupingElement>,
}

impl GroupingElement {
    /// The empty grouping set, `()`: the whole input as one group.
    ///
    /// On its own it is a grand total that answers one row even over an
    /// empty input; beside other sets it adds that total to their groups.
    pub fn empty() -> Self {
        Self {
            kind: GroupingKind::Set(Vec::new()),
        }
    }

    /// One grouping set of the given expressions, `("a", "b")`. A single
    /// expression renders bare, exactly as `add_group_by` writes it, and no
    /// expressions renders `()`, as [`empty`](Self::empty) does.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .column(Glyph::Aspect)
    ///     .from(Glyph::Table)
    ///     .group_by_element(
    ///         GroupingElement::sets(GroupingElement::set([
    ///             Expr::col(Glyph::Aspect),
    ///             Expr::col(Glyph::Image),
    ///         ]))
    ///         .add(GroupingElement::empty()),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "aspect" FROM "glyph" GROUP BY GROUPING SETS (("aspect", "image"), ())"#
    /// );
    /// ```
    pub fn set<I, T>(exprs: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<SimpleExpr>,
    {
        Self {
            kind: GroupingKind::Set(exprs.into_iter().map(Into::into).collect()),
        }
    }

    /// `ROLLUP ( … )`: grouping by every leading prefix of the list, longest
    /// first, down to `()` — `ROLLUP ("a", "b")` is the sets `("a", "b")`,
    /// `("a")` and `()`, the subtotals of a hierarchy.
    ///
    /// An [`Expr::tuple`](crate::Expr::tuple) item is one unit of the
    /// hierarchy, `ROLLUP (("a", "b"), "c")`, kept or dropped as a whole.
    /// With no expressions the only prefix is the empty one, so the element
    /// renders `()`, which is exactly that.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .column(Glyph::Aspect)
    ///     .expr(Func::grouping(Expr::col(Glyph::Aspect)).arg(Expr::col(Glyph::Image)))
    ///     .from(Glyph::Table)
    ///     .group_by_element(GroupingElement::rollup([
    ///         Expr::col(Glyph::Aspect),
    ///         Expr::col(Glyph::Image),
    ///     ]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "aspect", GROUPING("aspect", "image") FROM "glyph" GROUP BY ROLLUP ("aspect", "image")"#
    /// );
    /// ```
    pub fn rollup<I, T>(exprs: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<SimpleExpr>,
    {
        Self {
            kind: GroupingKind::Rollup(exprs.into_iter().map(Into::into).collect()),
        }
    }

    /// `CUBE ( … )`: grouping by every subset of the list, `()` included —
    /// `CUBE ("a", "b")` is the sets `("a", "b")`, `("a")`, `("b")` and `()`.
    ///
    /// Items are units as in [`rollup`](Self::rollup), and with no
    /// expressions the one subset is the empty one, so the element renders
    /// `()`.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .expr(Func::count(Expr::col(Glyph::Id)))
    ///     .from(Glyph::Table)
    ///     .group_by_element(GroupingElement::cube([
    ///         Expr::col(Glyph::Aspect),
    ///         Expr::col(Glyph::Image),
    ///     ]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT COUNT("id") FROM "glyph" GROUP BY CUBE ("aspect", "image")"#
    /// );
    /// ```
    pub fn cube<I, T>(exprs: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<SimpleExpr>,
    {
        Self {
            kind: GroupingKind::Cube(exprs.into_iter().map(Into::into).collect()),
        }
    }

    /// Starts `GROUPING SETS ( … )` with its first element; add the others
    /// with [`GroupingSets::add`]. The element stands for the sets of each of
    /// its elements in turn — a nested `ROLLUP` or `CUBE` contributing all of
    /// its own.
    pub fn sets(first: GroupingElement) -> GroupingSets {
        GroupingSets { sets: vec![first] }
    }
}

impl GroupingSets {
    /// Adds a further element to the list — `add`, as `Condition::add` adds a
    /// member to a condition set, rather than `std::ops::Add`.
    #[allow(clippy::should_implement_trait)]
    pub fn add(mut self, element: GroupingElement) -> Self {
        self.sets.push(element);
        self
    }
}

impl From<GroupingSets> for GroupingElement {
    fn from(sets: GroupingSets) -> Self {
        Self {
            kind: GroupingKind::Sets(sets.sets),
        }
    }
}

// `select.rs` is at its function cap, so the method that takes a grouping
// element sits here, beside the element.
impl SelectStatement {
    /// Adds a [`GroupingElement`] to the `GROUP BY` list, after anything
    /// already there. It composes with the plain group-by methods: the list
    /// groups by every combination of its items' sets, so
    /// `add_group_by([a])` followed by a `ROLLUP (b)` groups by `(a, b)` and
    /// `(a)`.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .column(Glyph::Aspect)
    ///     .from(Glyph::Table)
    ///     .group_by_col(Glyph::Aspect)
    ///     .group_by_element(GroupingElement::rollup([Expr::col(Glyph::Image)]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "aspect" FROM "glyph" GROUP BY "aspect", ROLLUP ("image")"#
    /// );
    /// ```
    // [spec:pgorm:def:sql.ast.select.grouping]
    pub fn group_by_element<G>(&mut self, element: G) -> &mut Self
    where
        G: Into<GroupingElement>,
    {
        self.groups.push(element.into());
        self
    }
}

/// `GROUPING(a, b, …)`: which of its arguments the current row's grouping set
/// leaves out, as a bitmask — the last argument is bit 0, and a bit is set
/// when that argument is *not* grouped, so a grand-total row over `n`
/// arguments answers `2ⁿ − 1`. It is how a query tells the NULL a subtotal
/// row puts in an omitted column from a NULL in the data.
///
/// Built by [`Func::grouping`], which takes the first argument, and extended
/// with [`arg`](Self::arg). It is not a [`FunctionCall`](crate::FunctionCall):
/// PostgreSQL's grammar spells it as its own expression, admitting neither
/// `FILTER`, `WITHIN GROUP`, `DISTINCT` nor `OVER`, and requiring at least one
/// argument, so none of those can be attached to it here. Each argument must
/// be one of the query's grouping expressions, which the server checks.
// [spec:pgorm:def:sql.ast.select.grouping]
#[derive(Debug, Clone, PartialEq)]
pub struct Grouping {
    pub(crate) args: Vec<SimpleExpr>,
}

impl Grouping {
    /// Adds a further argument, which takes the next lower bit: the last
    /// argument added is bit 0.
    pub fn arg<T>(mut self, arg: T) -> Self
    where
        T: Into<SimpleExpr>,
    {
        self.args.push(arg.into());
        self
    }
}

impl From<Grouping> for SimpleExpr {
    fn from(grouping: Grouping) -> Self {
        SimpleExpr::Grouping(grouping)
    }
}

// `GROUPING()` sits beside the grouping elements it reads rather than among
// the `Function` variants, because it is not a function call.
impl Func {
    /// `GROUPING(expr, …)`, starting from its first argument; add the others
    /// with [`Grouping::arg`]. See [`Grouping`] for what it answers.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .column(Glyph::Aspect)
    ///     .expr(Func::grouping(Expr::col(Glyph::Aspect)))
    ///     .expr(Func::count(Expr::col(Glyph::Id)))
    ///     .from(Glyph::Table)
    ///     .group_by_element(GroupingElement::rollup([Expr::col(Glyph::Aspect)]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "aspect", GROUPING("aspect"), COUNT("id") FROM "glyph" GROUP BY ROLLUP ("aspect")"#
    /// );
    /// ```
    // [spec:pgorm:def:sql.ast.select.grouping]
    pub fn grouping<T>(first: T) -> Grouping
    where
        T: Into<SimpleExpr>,
    {
        Grouping {
            args: vec![first.into()],
        }
    }
}
