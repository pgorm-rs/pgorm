use crate::{
    AnyWithClause, OnConflict, QueryStatementBuilder, ReturningClause, SelectStatement, SimpleExpr,
    SubQueryStatement, Values, WithQuery, backend::QueryBuilder, error::*, prepare::*, types::*,
};
use inherent::inherent;

/// Represents a value source that can be used in an insert query.
///
/// [`InsertValueSource`] is a node in the expression tree and can represent a raw value set
/// ('VALUES') or a select query.
// [spec:pgorm:def:sql.ast.insert+1]
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InsertValueSource {
    Values(Vec<Vec<SimpleExpr>>),
    Select(Box<SelectStatement>),
}

/// Insert any new rows into an existing table
///
/// # Examples
///
/// ```
/// use pgorm_query::{tests_cfg::*, *};
///
/// let query = Query::insert()
///     .into_table(Glyph::Table)
///     .columns([Glyph::Aspect, Glyph::Image])
///     .values_panic([5.15.into(), "12A".into()])
///     .values_panic([4.21.into(), "123".into()])
///     .to_owned();
///
/// assert_eq!(
///     query.to_string(),
///     r#"INSERT INTO "glyph" ("aspect", "image") VALUES (5.15, '12A'), (4.21, '123')"#
/// );
/// ```
// [spec:pgorm:def:sql.ast.insert+1]
#[derive(Debug, Default, Clone, PartialEq)]
pub struct InsertStatement {
    pub(crate) table: Option<NamedTable>,
    pub(crate) columns: Vec<DynIden>,
    pub(crate) source: Option<InsertValueSource>,
    pub(crate) on_conflict: Option<OnConflict>,
    pub(crate) returning: Option<ReturningClause>,
    pub(crate) default_values: Option<u32>,
}

impl InsertStatement {
    /// Construct a new [`InsertStatement`]
    pub fn new() -> Self {
        Self::default()
    }

    /// Specify which table to insert into.
    ///
    /// The target is a name, optionally aliased — the alias is what `ON
    /// CONFLICT DO UPDATE` refers back to:
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table.into_named_table().alias(Alias::new("g")))
    ///     .columns([Glyph::Image])
    ///     .values_panic(["12A".into()])
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" AS "g" ("image") VALUES ('12A')"#
    /// );
    /// ```
    ///
    /// # Examples
    ///
    /// See [`InsertStatement::values`]
    // [spec:pgorm:def:sql.ast.insert+1]
    pub fn into_table<T>(&mut self, tbl_ref: T) -> &mut Self
    where
        T: IntoNamedTable,
    {
        self.table = Some(tbl_ref.into_named_table());
        self
    }

    /// Specify what columns to insert.
    ///
    /// # Examples
    ///
    /// See [`InsertStatement::values`]
    pub fn columns<C, I>(&mut self, columns: I) -> &mut Self
    where
        C: IntoIden,
        I: IntoIterator<Item = C>,
    {
        self.columns = columns.into_iter().map(|c| c.into_iden()).collect();
        self
    }

    /// Specify a select query whose values to be inserted.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Aspect, Glyph::Image])
    ///     .select_from(Query::select()
    ///         .column(Glyph::Aspect)
    ///         .column(Glyph::Image)
    ///         .from(Glyph::Table)
    ///         .and_where(Expr::col(Glyph::Image).like("0%"))
    ///         .to_owned()
    ///     )
    ///     .unwrap()
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" ("aspect", "image") SELECT "aspect", "image" FROM "glyph" WHERE "image" LIKE '0%'"#
    /// );
    /// ```
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Image])
    ///     .select_from(
    ///         Query::select()
    ///             .expr(Expr::val("hello"))
    ///             .cond_where(Cond::all().not().add(Expr::exists(
    ///                 Query::select().expr(Expr::val("world")).to_owned(),
    ///             )))
    ///             .to_owned(),
    ///     )
    ///     .unwrap()
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" ("image") SELECT 'hello' WHERE NOT EXISTS(SELECT 'world')"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.insert.arity]
    pub fn select_from<S>(&mut self, select: S) -> Result<&mut Self>
    where
        S: Into<SelectStatement>,
    {
        let statement = select.into();

        if self.columns.len() != statement.selects.len() {
            return Err(Error::ColValNumMismatch {
                col_len: self.columns.len(),
                val_len: statement.selects.len(),
            });
        }

        self.source = Some(InsertValueSource::Select(Box::new(statement)));
        Ok(self)
    }

    /// Specify a row of values to be inserted.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Aspect, Glyph::Image])
    ///     .values([
    ///         2.into(),
    ///         Func::cast_as("2020-02-02 00:00:00", Alias::new("DATE")).into(),
    ///     ])
    ///     .unwrap()
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" ("aspect", "image") VALUES (2, CAST('2020-02-02 00:00:00' AS DATE))"#
    /// );
    /// ```
    // [spec:pgorm:req:sql.ast.insert.arity]
    pub fn values<I>(&mut self, values: I) -> Result<&mut Self>
    where
        I: IntoIterator<Item = SimpleExpr>,
    {
        let values = values.into_iter().collect::<Vec<SimpleExpr>>();
        if self.columns.len() != values.len() {
            return Err(Error::ColValNumMismatch {
                col_len: self.columns.len(),
                val_len: values.len(),
            });
        }
        if !values.is_empty() {
            let values_source = if let Some(InsertValueSource::Values(values)) = &mut self.source {
                values
            } else {
                self.source = Some(InsertValueSource::Values(Default::default()));
                if let Some(InsertValueSource::Values(values)) = &mut self.source {
                    values
                } else {
                    unreachable!();
                }
            };
            values_source.push(values);
        }
        Ok(self)
    }

    /// Specify a row of values to be inserted, variation of [`InsertStatement::values`].
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Aspect, Glyph::Image])
    ///     .values_panic([2.1345.into(), "24B".into()])
    ///     .values_panic([5.15.into(), "12A".into()])
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" ("aspect", "image") VALUES (2.1345, '24B'), (5.15, '12A')"#
    /// );
    /// ```
    pub fn values_panic<I>(&mut self, values: I) -> &mut Self
    where
        I: IntoIterator<Item = SimpleExpr>,
    {
        self.values(values).unwrap()
    }

    /// Add rows to be inserted from an iterator, variation of [`InsertStatement::values_panic`].
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let rows = vec![[2.1345.into(), "24B".into()], [5.15.into(), "12A".into()]];
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Aspect, Glyph::Image])
    ///     .values_from_panic(rows)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" ("aspect", "image") VALUES (2.1345, '24B'), (5.15, '12A')"#
    /// );
    /// ```
    pub fn values_from_panic<I>(&mut self, values_iter: impl IntoIterator<Item = I>) -> &mut Self
    where
        I: IntoIterator<Item = SimpleExpr>,
    {
        values_iter.into_iter().for_each(|values| {
            self.values_panic(values);
        });
        self
    }

    /// ON CONFLICT expression.
    ///
    /// Takes a finished [`OnConflict`], or a
    /// [`ConflictUpdate`](crate::ConflictUpdate) that converts into one, so a
    /// builder chain is passed as it stands. See [`OnConflict`] for the shapes
    /// available.
    pub fn on_conflict<T>(&mut self, on_conflict: T) -> &mut Self
    where
        T: Into<OnConflict>,
    {
        self.on_conflict = Some(on_conflict.into());
        self
    }

    /// Get the ON CONFLICT clause, if one was set.
    pub fn get_on_conflict(&self) -> Option<&OnConflict> {
        self.on_conflict.as_ref()
    }

    /// RETURNING expressions.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Image])
    ///     .values_panic(["12A".into()])
    ///     .returning(Query::returning().columns([Glyph::Id]))
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" ("image") VALUES ('12A') RETURNING "id""#
    /// );
    /// ```
    pub fn returning(&mut self, returning: ReturningClause) -> &mut Self {
        self.returning = Some(returning);
        self
    }

    /// RETURNING expressions for a column.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Image])
    ///     .values_panic(["12A".into()])
    ///     .returning_col(Glyph::Id)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" ("image") VALUES ('12A') RETURNING "id""#
    /// );
    /// ```
    pub fn returning_col<C>(&mut self, col: C) -> &mut Self
    where
        C: IntoColumnRef,
    {
        self.returning(ReturningClause::Columns(vec![col.into_column_ref()]))
    }

    /// RETURNING expressions all columns.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .columns([Glyph::Image])
    ///     .values_panic(["12A".into()])
    ///     .returning_all()
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" ("image") VALUES ('12A') RETURNING *"#
    /// );
    /// ```
    pub fn returning_all(&mut self) -> &mut Self {
        self.returning(ReturningClause::All)
    }

    /// Create a [WithQuery] by specifying a with clause to execute this query with. The clause is
    /// either a [`WithClause`](crate::WithClause) or a
    /// [`RecursiveWithClause`](crate::RecursiveWithClause).
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, IntoCondition, IntoIden, tests_cfg::*};
    ///
    /// let select = SelectStatement::new()
    ///         .columns([Glyph::Id, Glyph::Image, Glyph::Aspect])
    ///         .from(Glyph::Table)
    ///         .to_owned();
    ///     let cte = CommonTableExpression::new(Alias::new("cte"), select)
    ///         .column(Glyph::Id)
    ///         .column(Glyph::Image)
    ///         .column(Glyph::Aspect)
    ///         .to_owned();
    ///     let with_clause = WithClause::new(cte);
    ///     let select = SelectStatement::new()
    ///         .columns([Glyph::Id, Glyph::Image, Glyph::Aspect])
    ///         .from(Alias::new("cte"))
    ///         .to_owned();
    ///     let mut insert = Query::insert();
    ///     insert
    ///         .into_table(Glyph::Table)
    ///         .columns([Glyph::Id, Glyph::Image, Glyph::Aspect])
    ///         .select_from(select)
    ///         .unwrap();
    ///     let query = insert.with(with_clause);
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"WITH "cte" ("id", "image", "aspect") AS (SELECT "id", "image", "aspect" FROM "glyph") INSERT INTO "glyph" ("id", "image", "aspect") SELECT "id", "image", "aspect" FROM "cte""#
    /// );
    /// ```
    pub fn with<C>(self, clause: C) -> WithQuery
    where
        C: Into<AnyWithClause>,
    {
        WithQuery::new(clause, self)
    }

    /// Insert with default values if columns and values are not supplied.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// // Insert default
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .or_default_values()
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" VALUES (DEFAULT)"#
    /// );
    ///
    /// // Ordinary insert as columns and values are supplied
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .or_default_values()
    ///     .columns([Glyph::Image])
    ///     .values_panic(["ABC".into()])
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" ("image") VALUES ('ABC')"#
    /// );
    /// ```
    pub fn or_default_values(&mut self) -> &mut Self {
        self.default_values = Some(1);
        self
    }

    /// Insert multiple rows with default values if columns and values are not supplied.
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{tests_cfg::*, *};
    ///
    /// // Insert default
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .or_default_values_many(3)
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" VALUES (DEFAULT), (DEFAULT), (DEFAULT)"#
    /// );
    ///
    /// // Ordinary insert as columns and values are supplied
    /// let query = Query::insert()
    ///     .into_table(Glyph::Table)
    ///     .or_default_values_many(3)
    ///     .columns([Glyph::Image])
    ///     .values_panic(["ABC".into()])
    ///     .to_owned();
    ///
    /// assert_eq!(
    ///     query.to_string(),
    ///     r#"INSERT INTO "glyph" ("image") VALUES ('ABC')"#
    /// );
    /// ```
    pub fn or_default_values_many(&mut self, num_rows: u32) -> &mut Self {
        self.default_values = Some(num_rows);
        self
    }
}

#[inherent]
impl QueryStatementBuilder for InsertStatement {
    pub fn build_collect_into(&self, sql: &mut dyn SqlWriter) {
        QueryBuilder.prepare_insert_statement(self, sql);
    }

    pub fn into_sub_query_statement(self) -> SubQueryStatement {
        SubQueryStatement::InsertStatement(self)
    }

    pub fn build(&self) -> (String, Values);
    pub fn build_collect(&self, sql: &mut dyn SqlWriter) -> String;
}

// [spec:pgorm:req:sql.ast.build+1] (the one value-inlined rendering)
impl std::fmt::Display for InsertStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_insert_statement(self, &mut sql);
        f.write_str(&sql)
    }
}
