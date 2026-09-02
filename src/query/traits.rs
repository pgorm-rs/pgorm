use crate::{ColumnTrait, IntoIdentity, IntoSimpleExpr, QuerySelect};
use pgorm_query::{QueryStatementBuilder, Values};

/// A Trait for any type performing queries on a Model or ActiveModel
// [spec:pgorm:req:query.build]
// [spec:pgorm:def:query.build.query-trait]
pub trait QueryTrait {
    /// Constrain the QueryStatement to [QueryStatementBuilder] trait
    type QueryStatement: QueryStatementBuilder;

    /// Get a mutable ref to the query builder
    fn query(&mut self) -> &mut Self::QueryStatement;

    /// Get an immutable ref to the query builder
    fn as_query(&self) -> &Self::QueryStatement;

    /// Take ownership of the query builder
    fn into_query(self) -> Self::QueryStatement;

    /// Render the statement as PostgreSQL text plus its bound parameters.
    ///
    /// Values are never inlined: each one becomes a `$n` placeholder in the
    /// SQL string and an entry in [`Values`], in binding order. Use
    /// [`as_query`](QueryTrait::as_query) with
    /// [`to_string`](pgorm_query::QueryStatementWriter::to_string) instead
    /// when you want a self-contained, value-inlined string to read.
    ///
    /// # Example
    ///
    /// ```
    /// use pgorm::{entity::*, query::*, tests_cfg::cake};
    /// use pgorm::pgorm_query::{Value, Values};
    ///
    /// let (sql, values) = cake::Entity::find()
    ///     .filter(cake::Column::Id.eq(3))
    ///     .build();
    ///
    /// assert_eq!(
    ///     sql,
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" = $1"#
    /// );
    /// assert_eq!(values, Values(vec![Value::Int(Some(3))]));
    /// ```
    fn build(&self) -> (String, Values) {
        self.as_query().build_any(&pgorm_query::QueryBuilder)
    }

    /// Apply an operation on the [QueryTrait::QueryStatement] if the given `Option<T>` is `Some(_)`
    ///
    /// # Example
    ///
    /// ```
    /// use pgorm::{entity::*, pgorm_query::QueryBuilder, query::*, tests_cfg::cake};
    ///
    /// assert_eq!(
    ///     cake::Entity::find()
    ///         .apply_if(Some(3), |mut query, v| {
    ///             query.filter(cake::Column::Id.eq(v))
    ///         })
    ///         .apply_if(Some(100), QuerySelect::limit)
    ///         .apply_if(None, QuerySelect::offset::<Option<u64>>) // no-op
    ///         .as_query()
    ///         .to_string(QueryBuilder),
    ///     r#"SELECT "cake"."id", "cake"."name" FROM "cake" WHERE "cake"."id" = 3 LIMIT 100"#
    /// );
    /// ```
    fn apply_if<T, F>(self, val: Option<T>, if_some: F) -> Self
    where
        Self: Sized,
        F: FnOnce(Self, T) -> Self,
    {
        if let Some(val) = val {
            if_some(self, val)
        } else {
            self
        }
    }
}

/// Select specific column for partial model queries
// [spec:pgorm:sem:query.build.modifiers+2]
pub trait SelectColumns {
    /// The state this builder moves to once a column has been selected.
    ///
    /// The bound is a fixpoint: projecting an already-projected builder lands
    /// on the same type again, so a chain of any length has one type — and a
    /// chain of length *zero* does not, which is what makes a field-less
    /// `DerivePartialModel` fail to compile.
    type Projected: SelectColumns<Projected = Self::Projected>;

    /// Add a select column
    ///
    /// For more detail, please visit [QuerySelect::column]
    fn select_column<C: ColumnTrait>(self, col: C) -> Self::Projected;

    /// Add a select column with alias
    ///
    /// For more detail, please visit [QuerySelect::column_as]
    fn select_column_as<C, I>(self, col: C, alias: I) -> Self::Projected
    where
        C: IntoSimpleExpr,
        I: IntoIdentity;
}

impl<S> SelectColumns for S
where
    S: QuerySelect,
{
    type Projected = <S as QuerySelect>::Projected;

    fn select_column<C: ColumnTrait>(self, col: C) -> Self::Projected {
        QuerySelect::column(self, col)
    }

    fn select_column_as<C, I>(self, col: C, alias: I) -> Self::Projected
    where
        C: IntoSimpleExpr,
        I: IntoIdentity,
    {
        QuerySelect::column_as(self, col, alias)
    }
}
