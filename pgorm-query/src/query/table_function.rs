//! A set-returning function read as a relation: one call or several zipped
//! side by side under `ROWS FROM (..)`, optionally numbered `WITH
//! ORDINALITY`, under an alias that may name its columns.

use crate::{FromItem, FunctionCall, IntoName, Name};

/// A function in `FROM`, as PostgreSQL's `func_table` spells it: one call,
/// `f(..)`, or `ROWS FROM (f(..), g(..))`, which reads the calls' rows side
/// by side and pads the shorter ones with NULLs. [`with_ordinality`] adds a
/// final `bigint` column numbering the rows from 1, and
/// [`columns`](Self::columns) names the columns in the alias,
/// `AS "t"("a", "b")`, the ordinal column last.
///
/// [`alias`](Self::alias) finishes it as the [`FromItem`] that `from` and
/// the joins take, as [`JsonTable::alias`](crate::JsonTable::alias) does.
/// A lone call with no ordinal and no column names is exactly
/// [`FromItem::FunctionCall`], and `alias` returns it as one, so the two
/// spellings of `f(..) AS "t"` are one value.
///
/// # Examples
///
/// ```
/// use pgorm_query::*;
///
/// let series = Func::named(Name::runtime("generate_series")).arg(10).arg(12);
/// let query = Query::select()
///     .column(Asterisk)
///     .from(
///         TableFunction::new(series)
///             .with_ordinality()
///             .columns([Name::runtime("v"), Name::runtime("n")])
///             .alias(Name::runtime("t")),
///     )
///     .to_owned();
///
/// assert_eq!(
///     query.to_string(),
///     r#"SELECT * FROM generate_series(10, 12) WITH ORDINALITY AS "t"("v", "n")"#
/// );
/// ```
///
/// [`with_ordinality`]: Self::with_ordinality
// [spec:pgorm:req:sql.ast.select.table-function]
#[derive(Debug, Clone, PartialEq)]
pub struct TableFunction {
    pub(crate) functions: Vec<FunctionCall>,
    pub(crate) rows_from: bool,
    pub(crate) ordinality: bool,
    pub(crate) columns: Vec<Name>,
}

impl TableFunction {
    /// One set-returning call, `f(..)`.
    pub fn new(function: FunctionCall) -> Self {
        Self {
            functions: vec![function],
            rows_from: false,
            ordinality: false,
            columns: Vec::new(),
        }
    }

    /// `ROWS FROM (f(..))`, to which [`and`](Self::and) adds calls. Over one
    /// call it differs from [`new`](Self::new) only for a function returning
    /// a composite, whose columns `ROWS FROM` expands.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::*;
    ///
    /// let query = Query::select()
    ///     .column(Asterisk)
    ///     .from(
    ///         TableFunction::rows_from(
    ///             Func::named(Name::runtime("generate_series")).arg(1).arg(2),
    ///         )
    ///         .and(Func::named(Name::runtime("generate_series")).arg(1).arg(3))
    ///         .with_ordinality()
    ///         .columns([Name::runtime("x"), Name::runtime("y"), Name::runtime("n")])
    ///         .alias(Name::runtime("t")),
    ///     )
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT * FROM ROWS FROM (generate_series(1, 2), generate_series(1, 3)) WITH ORDINALITY AS "t"("x", "y", "n")"#
    /// );
    /// ```
    pub fn rows_from(function: FunctionCall) -> Self {
        Self {
            rows_from: true,
            ..Self::new(function)
        }
    }

    /// Add a call read beside the others. Several calls are `ROWS FROM`
    /// whichever constructor began it, since the grammar has no other
    /// spelling for them.
    pub fn and(mut self, function: FunctionCall) -> Self {
        self.functions.push(function);
        self.rows_from = true;
        self
    }

    /// Number the rows from 1 in a final `bigint` column: `WITH ORDINALITY`.
    pub fn with_ordinality(mut self) -> Self {
        self.ordinality = true;
        self
    }

    /// Name the relation's columns in its alias, `AS "t"("a", "b")`,
    /// replacing any names already given. The ordinal column, when there is
    /// one, comes last; naming fewer columns than the relation has leaves the
    /// rest their own names, and naming more is PostgreSQL's refusal
    /// (`42P10`).
    pub fn columns<I, N>(mut self, columns: I) -> Self
    where
        I: IntoIterator<Item = N>,
        N: IntoName,
    {
        self.columns = columns.into_iter().map(IntoName::into_name).collect();
        self
    }

    /// Finish the relation under `alias`, the [`FromItem`] `from` and the
    /// joins take.
    pub fn alias<A>(mut self, alias: A) -> FromItem
    where
        A: IntoName,
    {
        let alias = alias.into_name();
        if !self.rows_from
            && !self.ordinality
            && self.columns.is_empty()
            && let Some(function) = self.functions.pop()
        {
            return FromItem::FunctionCall(function, alias);
        }
        FromItem::TableFunction(Box::new(self), alias)
    }
}
