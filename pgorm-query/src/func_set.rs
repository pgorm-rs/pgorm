//! The set-returning functions a [`TableFunction`](crate::TableFunction)
//! reads as a relation: a series of numbers or instants, and an array's
//! elements as rows.
//!
//! Both are overloaded on their argument types, and a bound value is a
//! placeholder the server types from its context, of which an overloaded
//! call gives none: `GENERATE_SERIES($1, $2)` and `UNNEST($1)` are ambiguous
//! (`42725`). A bound argument is therefore cast to its type —
//! `Expr::val(1).cast_as(..)` — as anywhere a placeholder has no context
//! (`[spec:pgorm:sem:sql.render.placeholder-typing+1]`); a column or a typed
//! expression needs nothing.

use super::*;

impl Func {
    /// Call `GENERATE_SERIES(start, stop)`: every value from `start` to
    /// `stop` inclusive, a step of one apart — integers or numerics. A
    /// series of instants needs its interval step, which
    /// [`generate_series_step`](Self::generate_series_step) takes.
    ///
    /// ```
    /// use pgorm_query::*;
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .column(Asterisk)
    ///         .from(TableFunction::new(Func::generate_series(1, 3)).alias(Name::runtime("n")))
    ///         .to_string(),
    ///     r#"SELECT * FROM GENERATE_SERIES(1, 3) AS "n""#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.select.table-function]
    pub fn generate_series<S, E>(start: S, stop: E) -> FunctionCall
    where
        S: Into<SimpleExpr>,
        E: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::GenerateSeries)
            .arg(start)
            .arg(stop)
    }

    /// Call `GENERATE_SERIES(start, stop, step)`: the values from `start`
    /// towards `stop`, `step` apart — a negative step counts down, and over
    /// timestamps the step is an interval. A zero step is PostgreSQL's
    /// refusal (`22023`).
    // [spec:pgorm:req:sql.ast.select.table-function]
    pub fn generate_series_step<S, E, P>(start: S, stop: E, step: P) -> FunctionCall
    where
        S: Into<SimpleExpr>,
        E: Into<SimpleExpr>,
        P: Into<SimpleExpr>,
    {
        Self::generate_series(start, stop).arg(step)
    }

    /// Call `UNNEST(array)`: the array's elements as rows, in order — which
    /// `WITH ORDINALITY` numbers, giving each its position.
    ///
    /// ```
    /// use pgorm_query::*;
    ///
    /// assert_eq!(
    ///     Query::select()
    ///         .column(Asterisk)
    ///         .from(
    ///             TableFunction::new(Func::unnest(Expr::col(Name::runtime("tags"))))
    ///                 .with_ordinality()
    ///                 .alias(Name::runtime("t")),
    ///         )
    ///         .to_string(),
    ///     r#"SELECT * FROM UNNEST("tags") WITH ORDINALITY AS "t""#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.select.table-function]
    pub fn unnest<T>(array: T) -> FunctionCall
    where
        T: Into<SimpleExpr>,
    {
        FunctionCall::new(Function::Unnest).arg(array)
    }
}
