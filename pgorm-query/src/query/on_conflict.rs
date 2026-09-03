use crate::{Condition, ConditionHolder, DynIden, IntoCondition, IntoIden, SimpleExpr};

/// A complete `ON CONFLICT` clause.
///
/// PostgreSQL admits an arbiter-less clause only for `DO NOTHING`, so these two
/// variants are the two shapes it accepts: a bare `DO NOTHING`, or a conflict
/// target paired with the action taken on it. There is no clause without an
/// action to build, and no `DO UPDATE` without the target PostgreSQL demands
/// for one.
///
/// A clause is built by naming its target first, then its action:
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let query = Query::insert()
///     .into_table(Glyph::Table)
///     .columns([Glyph::Aspect])
///     .values_panic(["abcd".into()])
///     .on_conflict(OnConflict::column(Glyph::Id).do_nothing())
///     .to_owned();
///
/// assert_eq!(
///     query.to_string(),
///     r#"INSERT INTO "glyph" ("aspect") VALUES ('abcd') ON CONFLICT ("id") DO NOTHING"#
/// );
/// ```
///
/// A target under construction is not a clause, so a conflict target with no
/// action does not typecheck:
///
/// ```compile_fail,E0277
/// use pgorm_query::{tests_cfg::*, *};
///
/// Query::insert()
///     .into_table(Glyph::Table)
///     .columns([Glyph::Aspect])
///     .values_panic(["abcd".into()])
///     .on_conflict(OnConflict::column(Glyph::Id));
/// ```
///
/// `DO NOTHING` takes no `WHERE` — PostgreSQL accepts one only on `DO UPDATE` —
/// so a finished clause has nothing to attach a filter to:
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// OnConflict::column(Glyph::Id)
///     .do_nothing()
///     .and_where(Expr::col(Glyph::Aspect).gt(0));
/// ```
///
/// and the arbiter-less form reaches no action at all, so it cannot become the
/// targetless `DO UPDATE` PostgreSQL rejects:
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// OnConflict::do_nothing().update_column(Glyph::Aspect);
/// ```
// [spec:pgorm:req:sql.ast.on-conflict+1]
// The arbiter-less form genuinely carries nothing, so the size gap is the
// shape of the clause rather than a payload to box away.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum OnConflict {
    /// `ON CONFLICT DO NOTHING`: no inference specification, so a conflict on
    /// any constraint is swallowed.
    AnyDoNothing,
    /// `ON CONFLICT (..) DO ..`: a conflict target and the action taken on it.
    Targeted {
        /// Which conflicts this clause answers for.
        target: ConflictTarget,
        /// What is done about them.
        action: ConflictAction,
    },
}

/// One entry of a conflict target.
// [spec:pgorm:req:sql.ast.on-conflict+1]
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictElement {
    /// A column, as in `ON CONFLICT ("id")`.
    Column(DynIden),
    /// An expression, as in `ON CONFLICT (LOWER("name"))`.
    Expr(SimpleExpr),
}

/// The conflict target of an `ON CONFLICT` clause: the index inference
/// specification a conflict is matched against, and the optional partial-index
/// predicate narrowing it.
///
/// Non-empty by construction — [`OnConflict::column`] and [`OnConflict::expr`]
/// take the first entry and every extension adds one — so `ON CONFLICT ()`,
/// which the PostgreSQL grammar rejects, has no value to build. The predicate
/// lives here rather than beside the action because `ON CONFLICT WHERE ..` with
/// no target is rejected too.
///
/// There is no constructor taking a list, because a list can be empty:
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// OnConflict::columns([Glyph::Id, Glyph::Aspect]);
/// ```
// [spec:pgorm:req:sql.ast.on-conflict+1]
#[derive(Debug, Clone, PartialEq)]
pub struct ConflictTarget {
    pub(crate) first: ConflictElement,
    pub(crate) rest: Vec<ConflictElement>,
    pub(crate) filter: Option<Condition>,
}

/// One assignment of a `DO UPDATE SET`.
// [spec:pgorm:req:sql.ast.on-conflict+1]
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictAssignment {
    /// Take the column's value from the row that failed to insert:
    /// `"col" = "excluded"."col"`.
    Column(DynIden),
    /// Assign an expression: `"col" = <expr>`.
    Expr(DynIden, SimpleExpr),
}

/// The assignments of a `DO UPDATE SET`, non-empty by construction: the
/// transition into an update takes the first, so the dangling
/// `DO UPDATE SET ` with nothing after it has no value to build.
///
/// The plural forms extend an update in progress and so cannot begin one, which
/// is what keeps an empty list from becoming an empty `SET`:
///
/// ```compile_fail,E0599
/// use pgorm_query::{tests_cfg::*, *};
///
/// OnConflict::column(Glyph::Id).update_columns::<Glyph, _>([]);
/// ```
// [spec:pgorm:req:sql.ast.on-conflict+1]
#[derive(Debug, Clone, PartialEq)]
pub struct ConflictAssignments {
    pub(crate) first: ConflictAssignment,
    pub(crate) rest: Vec<ConflictAssignment>,
}

/// What an `ON CONFLICT` clause does about a conflict it matched.
///
/// Only `Update` carries a filter, because PostgreSQL accepts `WHERE` only
/// after `DO UPDATE SET ..`.
// [spec:pgorm:req:sql.ast.on-conflict+1]
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictAction {
    /// `DO NOTHING`.
    DoNothing,
    /// `DO UPDATE SET ..`.
    Update {
        /// The assignments, of which there is at least one.
        sets: ConflictAssignments,
        /// Restricts which conflicting rows are updated.
        filter: Option<Condition>,
    },
}

/// A `DO UPDATE SET` under construction, holding the target it answers for.
///
/// [`InsertStatement::on_conflict`](crate::InsertStatement::on_conflict) takes
/// anything that converts into an [`OnConflict`], so a chain ending here is
/// passed as it stands.
// [spec:pgorm:req:sql.ast.on-conflict+1]
#[derive(Debug, Clone, PartialEq)]
pub struct ConflictUpdate {
    target: ConflictTarget,
    sets: ConflictAssignments,
    filter: Option<Condition>,
}

/// Folds `addition` into `filter` through the same merge a `WHERE` clause uses.
fn merge(filter: Option<Condition>, addition: Condition) -> Option<Condition> {
    let mut holder = ConditionHolder { contents: filter };
    holder.add_condition(addition);
    holder.contents
}

impl OnConflict {
    /// `ON CONFLICT DO NOTHING`, with no inference specification: a conflict on
    /// any constraint is swallowed.
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Aspect])
    ///     .values_panic(["abcd".into()])
    ///     .on_conflict(OnConflict::do_nothing())
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" ("aspect") VALUES ('abcd') ON CONFLICT DO NOTHING"#
    /// );
    /// ```
    pub fn do_nothing() -> Self {
        Self::AnyDoNothing
    }

    /// Begin a conflict target at `column`.
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Aspect, Glyph::Image])
    ///     .values_panic([2.into(), 3.into()])
    ///     .on_conflict(
    ///         OnConflict::column(Glyph::Id)
    ///             .and_column(Glyph::Aspect)
    ///             .update_column(Glyph::Image),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     [
    ///         r#"INSERT INTO "glyph" ("aspect", "image") VALUES (2, 3)"#,
    ///         r#"ON CONFLICT ("id", "aspect") DO UPDATE SET "image" = "excluded"."image""#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    pub fn column<C>(column: C) -> ConflictTarget
    where
        C: IntoIden,
    {
        ConflictTarget::new(ConflictElement::Column(column.into_iden()))
    }

    /// Begin a conflict target at `expr`, for a conflict arbitrated by an
    /// expression index.
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Aspect, Glyph::Image])
    ///     .values_panic(["abcd".into(), 3.1415.into()])
    ///     .on_conflict(
    ///         OnConflict::expr(Expr::col(Glyph::Id))
    ///             .update_column(Glyph::Aspect)
    ///             .value(Glyph::Image, Expr::val(1).add(2)),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     [
    ///         r#"INSERT INTO "glyph" ("aspect", "image")"#,
    ///         r#"VALUES ('abcd', 3.1415)"#,
    ///         r#"ON CONFLICT ("id") DO UPDATE SET "aspect" = "excluded"."aspect", "image" = 1 + 2"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    pub fn expr<T>(expr: T) -> ConflictTarget
    where
        T: Into<SimpleExpr>,
    {
        ConflictTarget::new(ConflictElement::Expr(expr.into()))
    }
}

impl ConflictTarget {
    fn new(first: ConflictElement) -> Self {
        Self {
            first,
            rest: Vec::new(),
            filter: None,
        }
    }

    /// Add a further column to the target.
    #[must_use]
    pub fn and_column<C>(mut self, column: C) -> Self
    where
        C: IntoIden,
    {
        self.rest.push(ConflictElement::Column(column.into_iden()));
        self
    }

    /// Add further columns to the target. An empty iterator adds none, which
    /// leaves the target as it was rather than emptying it.
    #[must_use]
    pub fn and_columns<C, I>(mut self, columns: I) -> Self
    where
        C: IntoIden,
        I: IntoIterator<Item = C>,
    {
        self.rest.extend(
            columns
                .into_iter()
                .map(|c| ConflictElement::Column(c.into_iden())),
        );
        self
    }

    /// Add a further expression to the target.
    #[must_use]
    pub fn and_expr<T>(mut self, expr: T) -> Self
    where
        T: Into<SimpleExpr>,
    {
        self.rest.push(ConflictElement::Expr(expr.into()));
        self
    }

    /// Add further expressions to the target.
    #[must_use]
    pub fn and_exprs<T, I>(mut self, exprs: I) -> Self
    where
        T: Into<SimpleExpr>,
        I: IntoIterator<Item = T>,
    {
        self.rest
            .extend(exprs.into_iter().map(|e| ConflictElement::Expr(e.into())));
        self
    }

    /// The target's entries, in the order they were added.
    pub fn elements(&self) -> impl Iterator<Item = &ConflictElement> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }

    /// The partial-index predicate narrowing the arbiter, if one was given.
    pub fn filter(&self) -> Option<&Condition> {
        self.filter.as_ref()
    }

    /// Narrow the arbiter to a partial index's predicate.
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Aspect, Glyph::Image])
    ///     .values_panic([2.into(), 3.into()])
    ///     .on_conflict(
    ///         OnConflict::column(Glyph::Id)
    ///             .and_where(Expr::col((Glyph::Table, Glyph::Aspect)).is_null())
    ///             .value(Glyph::Image, Expr::val(1).add(2)),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     [
    ///         r#"INSERT INTO "glyph" ("aspect", "image") VALUES (2, 3)"#,
    ///         r#"ON CONFLICT ("id") WHERE "glyph"."aspect" IS NULL"#,
    ///         r#"DO UPDATE SET "image" = 1 + 2"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    #[must_use]
    pub fn and_where(self, other: SimpleExpr) -> Self {
        self.cond_where(other)
    }

    /// Narrow the arbiter when there is a predicate to narrow it by.
    #[must_use]
    pub fn and_where_option(self, other: Option<SimpleExpr>) -> Self {
        match other {
            Some(other) => self.cond_where(other),
            None => self,
        }
    }

    /// Narrow the arbiter by a condition.
    #[must_use]
    pub fn cond_where<C>(mut self, condition: C) -> Self
    where
        C: IntoCondition,
    {
        self.filter = merge(self.filter, condition.into_condition());
        self
    }

    /// Take no action on a conflict this target matches.
    pub fn do_nothing(self) -> OnConflict {
        OnConflict::Targeted {
            target: self,
            action: ConflictAction::DoNothing,
        }
    }

    /// Begin a `DO UPDATE SET` whose first assignment takes the column's value
    /// from the row that failed to insert.
    pub fn update_column<C>(self, column: C) -> ConflictUpdate
    where
        C: IntoIden,
    {
        ConflictUpdate::new(self, ConflictAssignment::Column(column.into_iden()))
    }

    /// Begin a `DO UPDATE SET` whose first assignment sets `col` to `value`.
    pub fn value<C, T>(self, col: C, value: T) -> ConflictUpdate
    where
        C: IntoIden,
        T: Into<SimpleExpr>,
    {
        ConflictUpdate::new(
            self,
            ConflictAssignment::Expr(col.into_iden(), value.into()),
        )
    }
}

impl ConflictAssignments {
    /// The assignments, in the order they were added.
    pub fn iter(&self) -> impl Iterator<Item = &ConflictAssignment> {
        std::iter::once(&self.first).chain(self.rest.iter())
    }
}

impl ConflictUpdate {
    fn new(target: ConflictTarget, first: ConflictAssignment) -> Self {
        Self {
            target,
            sets: ConflictAssignments {
                first,
                rest: Vec::new(),
            },
            filter: None,
        }
    }

    /// Also take this column's value from the row that failed to insert.
    #[must_use]
    pub fn update_column<C>(mut self, column: C) -> Self
    where
        C: IntoIden,
    {
        self.sets
            .rest
            .push(ConflictAssignment::Column(column.into_iden()));
        self
    }

    /// Also take these columns' values from the row that failed to insert.
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Aspect, Glyph::Image])
    ///     .values_panic([2.into(), 3.into()])
    ///     .on_conflict(
    ///         OnConflict::column(Glyph::Id)
    ///             .update_column(Glyph::Aspect)
    ///             .update_columns([Glyph::Image]),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     [
    ///         r#"INSERT INTO "glyph" ("aspect", "image") VALUES (2, 3)"#,
    ///         r#"ON CONFLICT ("id") DO UPDATE SET"#,
    ///         r#""aspect" = "excluded"."aspect", "image" = "excluded"."image""#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    #[must_use]
    pub fn update_columns<C, I>(mut self, columns: I) -> Self
    where
        C: IntoIden,
        I: IntoIterator<Item = C>,
    {
        self.sets.rest.extend(
            columns
                .into_iter()
                .map(|c| ConflictAssignment::Column(c.into_iden())),
        );
        self
    }

    /// Also set `col` to `value`.
    #[must_use]
    pub fn value<C, T>(self, col: C, value: T) -> Self
    where
        C: IntoIden,
        T: Into<SimpleExpr>,
    {
        self.values([(col, value.into())])
    }

    /// Also apply these `(column, expression)` assignments.
    #[must_use]
    pub fn values<C, I>(mut self, values: I) -> Self
    where
        C: IntoIden,
        I: IntoIterator<Item = (C, SimpleExpr)>,
    {
        self.sets.rest.extend(
            values
                .into_iter()
                .map(|(c, e)| ConflictAssignment::Expr(c.into_iden(), e)),
        );
        self
    }

    /// Restrict which conflicting rows the update reaches.
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Aspect, Glyph::Image])
    ///     .values_panic([2.into(), 3.into()])
    ///     .on_conflict(
    ///         OnConflict::column(Glyph::Id)
    ///             .value(Glyph::Image, Expr::val(1).add(2))
    ///             .and_where(Expr::col((Glyph::Table, Glyph::Aspect)).is_null()),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     [
    ///         r#"INSERT INTO "glyph" ("aspect", "image") VALUES (2, 3)"#,
    ///         r#"ON CONFLICT ("id") DO UPDATE SET "image" = 1 + 2"#,
    ///         r#"WHERE "glyph"."aspect" IS NULL"#,
    ///     ]
    ///     .join(" ")
    /// );
    /// ```
    #[must_use]
    pub fn and_where(self, other: SimpleExpr) -> Self {
        self.cond_where(other)
    }

    /// Restrict the update when there is a predicate to restrict it by.
    #[must_use]
    pub fn and_where_option(self, other: Option<SimpleExpr>) -> Self {
        match other {
            Some(other) => self.cond_where(other),
            None => self,
        }
    }

    /// Restrict the update by a condition.
    #[must_use]
    pub fn cond_where<C>(mut self, condition: C) -> Self
    where
        C: IntoCondition,
    {
        self.filter = merge(self.filter, condition.into_condition());
        self
    }
}

impl From<ConflictUpdate> for OnConflict {
    fn from(update: ConflictUpdate) -> Self {
        Self::Targeted {
            target: update.target,
            action: ConflictAction::Update {
                sets: update.sets,
                filter: update.filter,
            },
        }
    }
}
