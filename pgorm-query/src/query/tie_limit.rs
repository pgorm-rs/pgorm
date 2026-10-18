//! `FETCH FIRST n ROWS WITH TIES`: a row limit that keeps every row tied with
//! the last one it counts, and the check that refuses the two places
//! PostgreSQL's grammar will not take one.

use crate::{
    LockBehavior, SelectStatement, Value,
    error::{Error, Result},
};

/// A SELECT's row limit. The two forms share one slot because PostgreSQL's
/// grammar takes a `LIMIT` or a `FETCH FIRST`, never both, so setting either
/// replaces the other.
// [spec:pgorm:req:sql.ast.select.tie-limit]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum RowLimit {
    /// `LIMIT n`: at most `n` rows.
    Count(Value),
    /// `FETCH FIRST n ROWS WITH TIES`: the first `n` rows and every further
    /// row equal to the `n`-th on the ORDER BY keys.
    WithTies(Value),
}

impl SelectStatement {
    /// Limit the result to its first `count` rows *and every row tied with the
    /// last of them* on the ORDER BY keys: `FETCH FIRST $n ROWS WITH TIES`.
    /// It replaces any row limit already set, a plain [`limit`](Self::limit)
    /// included, and [`reset_limit`](Self::reset_limit) removes it.
    ///
    /// The ties are measured on the statement's ORDER BY, so a tie limit
    /// needs one, and PostgreSQL's grammar refuses `WITH TIES` beside
    /// `SKIP LOCKED`. Both are checked by
    /// [`check_tie_limit`](Self::check_tie_limit) rather than at this call,
    /// because a statement is built in any order and its ORDER BY can be
    /// added after the limit (or cleared before it renders).
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .column(Glyph::Image)
    ///     .from(Glyph::Table)
    ///     .order_by(Glyph::Aspect, Order::Desc)
    ///     .limit_with_ties(3)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"SELECT "image" FROM "glyph" ORDER BY "aspect" DESC FETCH FIRST 3 ROWS WITH TIES"#
    /// );
    /// assert_eq!(query.check_tie_limit(), Ok(()));
    /// ```
    // [spec:pgorm:req:sql.ast.select.tie-limit]
    pub fn limit_with_ties(&mut self, count: u64) -> &mut Self {
        self.limit = Some(RowLimit::WithTies(count.into()));
        self
    }

    /// Whether the row limit is a tie limit, which counts rows only up to
    /// the ties of the last: a caller that pages or narrows the result by
    /// row count reads this before replacing the limit.
    // [spec:pgorm:req:sql.ast.select.tie-limit]
    pub fn has_tie_limit(&self) -> bool {
        matches!(self.limit, Some(RowLimit::WithTies(_)))
    }

    /// Refuse a tie limit PostgreSQL's grammar would refuse (`42601`): one on
    /// a statement with no ORDER BY to measure its ties on
    /// ([`Error::TiesWithoutOrder`]), and one beside `SKIP LOCKED`
    /// ([`Error::TiesWithSkipLocked`]). A statement without a tie limit
    /// always passes.
    ///
    /// Rendering is infallible, so it writes what the statement holds and
    /// leaves the refusal to this check; pgorm's executors and the Python
    /// and JavaScript bindings call it before they render. It reads this
    /// statement's own clauses only: a SELECT nested in it is checked by the
    /// server.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, error::Error, tests_cfg::*};
    ///
    /// let mut query = Query::select();
    /// query.column(Glyph::Image).from(Glyph::Table).limit_with_ties(3);
    /// assert_eq!(query.check_tie_limit(), Err(Error::TiesWithoutOrder));
    ///
    /// query.order_by(Glyph::Aspect, Order::Asc);
    /// assert_eq!(query.check_tie_limit(), Ok(()));
    ///
    /// query.lock_with_behavior(LockType::Update, LockBehavior::SkipLocked);
    /// assert_eq!(query.check_tie_limit(), Err(Error::TiesWithSkipLocked));
    /// ```
    // [spec:pgorm:req:sql.ast.select.tie-limit]
    pub fn check_tie_limit(&self) -> Result<()> {
        if !self.has_tie_limit() {
            Ok(())
        } else if self.orders.is_empty() {
            Err(Error::TiesWithoutOrder)
        } else if self
            .lock
            .as_ref()
            .is_some_and(|lock| lock.behavior == Some(LockBehavior::SkipLocked))
        {
            Err(Error::TiesWithSkipLocked)
        } else {
            Ok(())
        }
    }
}
