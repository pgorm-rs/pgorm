use crate::{
    ConnectionTrait, EntityTrait, FromQueryResult, Select, SelectModel, Selector, SelectorRaw,
    SelectorTrait, error::*,
};
use async_stream::stream;
use futures::Stream;
use pg_query::{
    NodeEnum,
    protobuf::{RawStmt, Token as ScanToken},
};
use pgorm_query::{Expr, SelectStatement, Value, Values, alias};
use std::{
    fmt::{self, Write as _},
    marker::PhantomData,
    num::NonZeroU64,
    pin::Pin,
};
use tokio_postgres::types::ToSql;

use super::{QueryResult, ValueHolder, select::ensure_select_list};

/// Pin a Model so that stream operations can be performed on the model
pub type PinBoxStream<'db, Item> = Pin<Box<dyn Stream<Item = Item> + 'db>>;

/// Defined a structure to handle pagination of a result from a query operation on a Model
// [spec:pgorm:def:exec.paginator+2]
#[derive(Clone, Debug)]
pub struct Paginator<'db, C, S>
where
    C: ConnectionTrait,
    S: SelectorTrait + 'db,
{
    pub(crate) query: Result<PagedQuery, String>,
    pub(crate) page: u64,
    pub(crate) page_size: NonZeroU64,
    pub(crate) db: &'db C,
    pub(crate) selector: PhantomData<S>,
}

/// Define a structure containing the numbers of items and pages of a Paginator
#[derive(Clone, Debug)]
pub struct ItemsAndPagesNumber {
    /// The total number of items of a paginator
    pub number_of_items: u64,
    /// The total number of pages of a paginator
    pub number_of_pages: u64,
}

/// The statement a paginator pages over, in the two forms that differ in who
/// owns the parameter numbering.
///
/// A statement pgorm-query built is numbered by the builder, so `LIMIT` and
/// `OFFSET` can simply be added to it and the whole thing rebuilt. A caller's
/// own statement is numbered by the caller: its `$N` markers count the caller's
/// own values, and rewriting that text to renumber them means re-lexing SQL
/// pgorm did not write. So a raw statement is held as text and never re-lexed —
/// pagination appends markers that continue the caller's numbering instead.
///
/// The two variants are lopsided by the size of a `SelectStatement`, which a
/// `Paginator` held inline before there was a second form to hold; boxing would
/// buy an allocation and a deref on the common path for a value that lives one
/// to a paginator.
// [spec:pgorm:sem:exec.paginator.raw+3]
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug)]
pub(crate) enum PagedQuery {
    /// A statement pgorm-query built and will build again with the page clauses.
    Built(SelectStatement),
    /// A caller's own statement, already wrapped as a pageable subquery.
    Raw(RawStatement),
}

/// A caller's statement wrapped as `SELECT * FROM (<statement>) AS
/// `[`RAW_SUBQUERY_ALIAS`], held as the text that will be sent alongside the
/// values its own `$N` markers number.
// [spec:pgorm:sem:exec.paginator.raw+3]
#[derive(Clone, Debug)]
pub(crate) struct RawStatement {
    sql: String,
    values: Vec<Value>,
}

/// The projection and alias `num_items` counts through, shared by both forms so
/// the count query reads the same whichever one it wrapped.
const COUNT_PROJECTION: &str = "COUNT(*) AS num_items";
const COUNT_SUBQUERY_ALIAS: &str = "sub_query";

impl PagedQuery {
    /// The statement for one page and the values to bind to it.
    // [spec:pgorm:sem:exec.paginator.raw+3]
    fn page(&self, limit: u64, offset: u64) -> Result<(String, Values), Error> {
        match self {
            Self::Built(query) => {
                ensure_select_list(query)?;
                let mut query = query.clone();
                query.limit(limit).offset(offset);
                Ok(query.build())
            }
            // The caller's markers run `$1..$N`; the page clauses continue from
            // `$N+1`, which is what makes the caller's text safe to send whole.
            Self::Raw(raw) => {
                let bound = raw.values.len();
                let mut values = raw.values.clone();
                values.push(limit.into());
                values.push(offset.into());
                Ok((
                    format!("{} LIMIT ${} OFFSET ${}", raw.sql, bound + 1, bound + 2),
                    Values(values),
                ))
            }
        }
    }

    /// The statement counting every row the paginator pages over, and the values
    /// to bind to it.
    // [spec:pgorm:sem:exec.paginator.raw+3]
    fn count(&self) -> Result<(String, Values), Error> {
        match self {
            Self::Built(query) => {
                ensure_select_list(query)?;
                let mut counted = query.clone();
                counted.reset_limit().reset_offset().clear_order_by();
                Ok(SelectStatement::new()
                    .expr(Expr::cust(COUNT_PROJECTION))
                    .from_subquery(counted, alias(COUNT_SUBQUERY_ALIAS))
                    .to_owned()
                    .build())
            }
            Self::Raw(raw) => Ok((
                format!(
                    r#"SELECT {COUNT_PROJECTION} FROM ({}) AS "{COUNT_SUBQUERY_ALIAS}""#,
                    raw.sql
                ),
                Values(raw.values.clone()),
            )),
        }
    }
}

// LINT: warn if paginator is used without an order by clause

impl<'db, C, S> Paginator<'db, C, S>
where
    C: ConnectionTrait,
    S: SelectorTrait + 'db,
{
    /// The statement to page over, or the reason there is none to page over.
    // [spec:pgorm:sem:exec.paginator.raw+3]
    fn query(&self) -> Result<&PagedQuery, Error> {
        self.query
            .as_ref()
            .map_err(|report| Error::Query(RuntimeError::Internal(report.clone())))
    }

    /// Fetch a specific page; page index starts from zero
    // [spec:pgorm:sem:exec.paginator.fetch+2]
    pub async fn fetch_page(&self, page: u64) -> Result<Vec<S::Item>, Error> {
        let offset = self.page_size.get().checked_mul(page).ok_or_else(|| {
            Error::Query(RuntimeError::Internal(format!(
                "page {page} at page size {} is past the largest representable offset",
                self.page_size
            )))
        })?;
        let (stmt, values) = self.query()?.page(self.page_size.get(), offset)?;
        let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
        let values = values
            .iter()
            .map(|x| x as _)
            .collect::<Vec<&(dyn ToSql + Sync)>>();
        let rows = self.db.query_all(&stmt, &values).await?;
        let mut buffer = Vec::with_capacity(rows.len());
        for row in rows.into_iter() {
            // TODO: Error handling
            buffer.push(S::from_raw_query_result(QueryResult { row })?);
        }
        Ok(buffer)
    }

    /// Fetch the current page
    // [spec:pgorm:sem:exec.paginator.fetch+2]
    pub async fn fetch(&self) -> Result<Vec<S::Item>, Error> {
        self.fetch_page(self.page).await
    }

    /// Get the total number of items
    // [spec:pgorm:sem:exec.paginator.count]
    pub async fn num_items(&self) -> Result<u64, Error> {
        let (stmt, values) = self.query()?.count()?;
        let values = values.into_iter().map(ValueHolder).collect::<Vec<_>>();
        let values = values
            .iter()
            .map(|x| x as _)
            .collect::<Vec<&(dyn ToSql + Sync)>>();
        let result = match self.db.query_opt(&stmt, &values).await? {
            Some(res) => res,
            None => return Ok(0),
        };
        let result = QueryResult { row: result };
        let num_items = result.try_get::<i64>("", "num_items")? as u64;
        Ok(num_items)
    }

    /// Get the total number of pages
    pub async fn num_pages(&self) -> Result<u64, Error> {
        let num_items = self.num_items().await?;
        let num_pages = self.compute_pages_number(num_items);
        Ok(num_pages)
    }

    /// Get the total number of items and pages
    pub async fn num_items_and_pages(&self) -> Result<ItemsAndPagesNumber, Error> {
        let number_of_items = self.num_items().await?;
        let number_of_pages = self.compute_pages_number(number_of_items);

        Ok(ItemsAndPagesNumber {
            number_of_items,
            number_of_pages,
        })
    }

    /// Compute the number of pages for the current page
    // [spec:pgorm:sem:exec.paginator.count]
    fn compute_pages_number(&self, num_items: u64) -> u64 {
        (num_items / self.page_size) + (num_items % self.page_size > 0) as u64
    }

    /// Increment the page counter
    pub fn next(&mut self) {
        self.page += 1;
    }

    /// Get current page number
    pub fn cur_page(&self) -> u64 {
        self.page
    }

    /// Fetch one page and increment the page counter
    ///
    /// Yields `None` once a page comes back empty.
    ///
    /// ```no_run
    /// # use std::num::NonZeroU64;
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool, PaginatorTrait};
    /// #
    /// # const PAGE_SIZE: NonZeroU64 = NonZeroU64::new(50).unwrap();
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// let db = pool.get().await?;
    ///
    /// let mut cake_pages = cake::Entity::find()
    ///     .order_by_asc(cake::Column::Id)
    ///     .paginate(&db, PAGE_SIZE);
    ///
    /// while let Some(cakes) = cake_pages.fetch_and_next().await? {
    ///     // Do something on cakes: Vec<cake::Model>
    /// }
    /// # Ok(())
    /// # }
    /// ```
    // [spec:pgorm:sem:exec.paginator.iterate]
    pub async fn fetch_and_next(&mut self) -> Result<Option<Vec<S::Item>>, Error> {
        let vec = self.fetch().await?;
        self.next();
        let opt = if !vec.is_empty() { Some(vec) } else { None };
        Ok(opt)
    }

    /// Convert self into an async stream
    ///
    /// ```no_run
    /// # use std::num::NonZeroU64;
    /// # use futures::TryStreamExt;
    /// # use pgorm::{entity::*, error::*, query::*, tests_cfg::cake, DatabasePool, PaginatorTrait};
    /// #
    /// # const PAGE_SIZE: NonZeroU64 = NonZeroU64::new(50).unwrap();
    /// # async fn example(pool: &DatabasePool) -> Result<(), Error> {
    /// let db = pool.get().await?;
    ///
    /// let mut cake_stream = cake::Entity::find()
    ///     .order_by_asc(cake::Column::Id)
    ///     .paginate(&db, PAGE_SIZE)
    ///     .into_stream();
    ///
    /// while let Some(cakes) = cake_stream.try_next().await? {
    ///     // Do something on cakes: Vec<cake::Model>
    /// }
    /// # Ok(())
    /// # }
    /// ```
    // [spec:pgorm:sem:exec.paginator.iterate]
    pub fn into_stream(mut self) -> PinBoxStream<'db, Result<Vec<S::Item>, Error>> {
        Box::pin(stream! {
            while let Some(vec) = self.fetch_and_next().await? {
                yield Ok(vec);
            }
        })
    }
}

#[async_trait::async_trait]
/// A Trait for any type that can paginate results
// [spec:pgorm:def:exec.paginator+2]
pub trait PaginatorTrait<'db, C>
where
    C: ConnectionTrait,
{
    /// Select operation
    type Selector: SelectorTrait + Send + Sync + 'db;

    /// Paginate the result of a select operation.
    ///
    /// A zero page size — which would make every page empty and leave the
    /// page count undefined — is not a value this can be called with:
    ///
    /// ```compile_fail,E0308
    /// # use pgorm::{entity::prelude::*, tests_cfg::cake, DatabaseConnection, PaginatorTrait};
    /// # fn example(db: &DatabaseConnection) {
    /// cake::Entity::find().paginate(db, 0);
    /// # }
    /// ```
    ///
    /// A page size known at compile time is checked there, once:
    ///
    /// ```
    /// # use std::num::NonZeroU64;
    /// # use pgorm::{entity::prelude::*, tests_cfg::cake, DatabaseConnection, PaginatorTrait};
    /// const PAGE_SIZE: NonZeroU64 = NonZeroU64::new(50).unwrap();
    ///
    /// # fn example(db: &DatabaseConnection) {
    /// cake::Entity::find().paginate(db, PAGE_SIZE);
    /// # }
    /// ```
    // [spec:pgorm:req:exec.paginator.page-size+2/test]
    fn paginate(self, db: &'db C, page_size: NonZeroU64) -> Paginator<'db, C, Self::Selector>;

    /// Perform a count on the paginated results
    async fn count(self, db: &'db C) -> Result<u64, Error>
    where
        Self: Send + Sized,
    {
        self.paginate(db, NonZeroU64::MIN).num_items().await
    }
}

impl<'db, C, S> PaginatorTrait<'db, C> for Selector<S>
where
    C: ConnectionTrait,
    S: SelectorTrait + Send + Sync + 'db,
{
    type Selector = S;

    // [spec:pgorm:req:exec.paginator.page-size+2]
    fn paginate(self, db: &'db C, page_size: NonZeroU64) -> Paginator<'db, C, S> {
        Paginator {
            query: Ok(PagedQuery::Built(self.query)),
            page: 0,
            page_size,
            db,
            selector: PhantomData,
        }
    }
}

impl<'db, C, S> PaginatorTrait<'db, C> for SelectorRaw<S>
where
    C: ConnectionTrait,
    S: SelectorTrait + Send + Sync + 'db,
{
    type Selector = S;
    // [spec:pgorm:req:exec.paginator.page-size+2]
    // [spec:pgorm:sem:exec.paginator.raw+3]
    fn paginate(self, db: &'db C, page_size: NonZeroU64) -> Paginator<'db, C, S> {
        Paginator {
            query: wrap_raw_select(&self.stmt, self.values.0),
            page: 0,
            page_size,
            db,
            selector: PhantomData,
        }
    }
}

/// The alias the caller's own statement is paged over as.
const RAW_SUBQUERY_ALIAS: &str = "sub_statement";

/// Everything the wrapper writes after the caller's statement. The newline is
/// Wrap a caller's raw statement as `SELECT * FROM (<statement>) AS "sub_statement"`,
/// the shape `LIMIT` and `OFFSET` append to whatever clauses the statement carries
/// of its own, or report why it cannot be paged over at all.
///
/// The statement's own text is copied, never rewritten: its `$N` markers already
/// number `values` and keep those numbers, because nothing is bound ahead of it.
/// The newline before the closing parenthesis is load-bearing — a statement
/// ending in a `--` comment would otherwise swallow the parenthesis into it.
// [spec:pgorm:sem:exec.paginator.raw+3]
fn wrap_raw_select(stmt: &str, values: Vec<Value>) -> Result<PagedQuery, String> {
    let select = single_select(stmt)?;
    check_markers(select, values.len())?;
    Ok(PagedQuery::Raw(RawStatement {
        sql: format!("SELECT * FROM ({select}\n) AS \"{RAW_SUBQUERY_ALIAS}\""),
        values,
    }))
}

/// Refuse a statement that reads a value it was never given, before any of it
/// reaches the server.
///
/// Which `$N` are parameter markers is PostgreSQL's own scanner's answer rather
/// than a guess made from the text, so a `$99` inside a comment, a
/// dollar-quoted body or a string literal is comment or string text and binds
/// nothing.
// [spec:pgorm:sem:exec.paginator.raw+3]
fn check_markers(select: &str, bound: usize) -> Result<(), String> {
    let scanned = pg_query::scan(select).map_err(|error| {
        format!(
            "cannot paginate a raw statement PostgreSQL cannot scan: {}",
            parser_message(&error)
        )
    })?;

    for token in &scanned.tokens {
        if token.token != ScanToken::Param as i32 {
            continue;
        }

        let start = usize::try_from(token.start).unwrap_or(0);
        let end = usize::try_from(token.end).unwrap_or(0);
        let marker = select.get(start..end).ok_or_else(|| {
            "cannot paginate a raw statement PostgreSQL located a marker outside".to_owned()
        })?;

        let number = marker
            .strip_prefix('$')
            .and_then(|digits| digits.parse::<usize>().ok())
            .ok_or_else(|| {
                format!("cannot paginate a raw statement whose marker {marker} is not numbered")
            })?;

        if number > bound {
            return Err(format!(
                "cannot paginate a raw statement reading {marker} when {bound} bind {} supplied",
                if bound == 1 {
                    "value was"
                } else {
                    "values were"
                }
            ));
        }
    }

    Ok(())
}

/// The one row-returning `SELECT` in `stmt`, at the extent libpg_query reports
/// for it — which excludes any terminating `;` a subquery position would refuse.
///
/// A `WITH ... SELECT` qualifies: PostgreSQL hangs the `WITH` clause off the
/// `SelectStmt` itself rather than making it a statement of its own.
// [spec:pgorm:sem:exec.paginator.raw+3]
fn single_select(stmt: &str) -> Result<&str, String> {
    let parsed = pg_query::parse(stmt).map_err(|error| {
        format!(
            "cannot paginate a raw statement PostgreSQL rejects: {}",
            parser_message(&error)
        )
    })?;

    let [raw] = parsed.protobuf.stmts.as_slice() else {
        return Err(format!(
            "cannot paginate a raw statement holding {} statements; pagination needs exactly one SELECT",
            parsed.protobuf.stmts.len()
        ));
    };

    let node = raw
        .stmt
        .as_ref()
        .and_then(|node| node.node.as_ref())
        .ok_or_else(|| "cannot paginate a raw statement that parsed to nothing".to_owned())?;

    match node {
        NodeEnum::SelectStmt(select) if select.into_clause.is_some() => Err(
            "cannot paginate a raw SELECT ... INTO; it creates a table rather than returning rows"
                .to_owned(),
        ),
        NodeEnum::SelectStmt(_) => extent(stmt, raw),
        other => Err(format!(
            "cannot paginate a raw statement that parses as {}; pagination needs a SELECT",
            node_name(other)
        )),
    }
}

/// The slice of `sql` that `raw` covers. `stmt_len` is zero for a statement
/// running to the end of the input with nothing terminating it.
fn extent<'sql>(sql: &'sql str, raw: &RawStmt) -> Result<&'sql str, String> {
    let start = usize::try_from(raw.stmt_location).unwrap_or(0);
    let end = match usize::try_from(raw.stmt_len) {
        Ok(0) | Err(_) => sql.len(),
        Ok(len) => start.saturating_add(len),
    };

    sql.get(start..end)
        .ok_or_else(|| "cannot paginate a raw statement PostgreSQL located outside it".to_owned())
}

/// PostgreSQL's own name for a parse node — `InsertStmt`, `VariableSetStmt` —
/// read off the head of its `Debug` form without rendering the tree beneath it.
fn node_name(node: &NodeEnum) -> String {
    let mut name = Head(String::new());
    let _ = write!(name, "{node:?}");
    name.0
}

/// A sink that keeps what a formatter writes before the first `(` and then stops
/// it, so a node's name costs nothing but the name.
struct Head(String);

impl fmt::Write for Head {
    fn write_str(&mut self, chunk: &str) -> fmt::Result {
        match chunk.split_once('(') {
            Some((head, _)) => {
                self.0.push_str(head);
                Err(fmt::Error)
            }
            None => {
                self.0.push_str(chunk);
                Ok(())
            }
        }
    }
}

/// The parser's own diagnostic, unwrapped from the `pg_query` error variant that
/// would otherwise prefix it with a stage name.
fn parser_message(error: &pg_query::Error) -> String {
    match error {
        pg_query::Error::Parse(message) | pg_query::Error::Scan(message) => message.clone(),
        other => other.to_string(),
    }
}

impl<'db, C, M, E> PaginatorTrait<'db, C> for Select<E>
where
    C: ConnectionTrait,
    E: EntityTrait<Model = M>,
    M: FromQueryResult + Sized + Send + Sync + 'db,
{
    type Selector = SelectModel<M>;

    fn paginate(self, db: &'db C, page_size: NonZeroU64) -> Paginator<'db, C, Self::Selector> {
        self.into_model().paginate(db, page_size)
    }
}

// [spec:pgorm:sem:exec.paginator.raw+3/test]    a caller's statement survives
// wrapping byte for byte, whatever token forms its text is made of, and a
// marker with no value behind it is refused rather than indexed
#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    /// Token forms a `$N` walk over the text would corrupt, each with the number
    /// of values it binds. Every one holds a `$99` that is *not* a marker — it
    /// is comment or string text — so a walk that substituted it would index
    /// past the values, and a walk that rewrote the text around it would change
    /// what the statement says.
    const TOKEN_FORMS: &[(&str, usize)] = &[
        ("SELECT $1::int4 AS n /* $99 is only a comment */", 1),
        // PostgreSQL nests block comments, so the inner close does not end it.
        ("SELECT $1::int4 AS n /* outer /* $99 */ still outer */", 1),
        ("SELECT $1::int4 AS n -- $99 to end of line", 1),
        (r#"SELECT $1::int4 AS n, $$hello $99 '"` $$ AS msg"#, 1),
        (
            "SELECT $1::int4 AS n, $tag$body $99 $$ inside$tag$ AS msg",
            1,
        ),
        ("SELECT $1::int4 AS n, 'it''s $99' AS msg", 1),
        (r"SELECT $1::int4 AS n, E'a\'b $99' AS msg", 1),
        // The same value read twice, and two values read out of order.
        ("SELECT $1::int4 AS a, $1::int4 AS b", 1),
        ("SELECT $2::int4 AS a, $1::int4 AS b", 2),
        // Subscripts: the brackets are not a quoted region, and the markers
        // inside them are markers.
        ("SELECT (ARRAY[$1::int4, 8])[$2::int4] AS n", 2),
        ("SELECT ($1::int4[])[1] AS n", 1),
    ];

    /// The `RawStatement` a statement carrying `bound` values wraps to.
    fn wrapped(stmt: &str, bound: usize) -> Result<RawStatement, String> {
        let values = vec![Value::Int(Some(7)); bound];
        match wrap_raw_select(stmt, values)? {
            PagedQuery::Raw(raw) => Ok(raw),
            PagedQuery::Built(_) => Err("a raw statement wrapped as a built one".to_owned()),
        }
    }

    #[test]
    fn keeps_every_token_form_verbatim() {
        for (stmt, bound) in TOKEN_FORMS {
            let raw = match wrapped(stmt, *bound) {
                Ok(raw) => raw,
                Err(report) => panic!("{stmt:?} was refused: {report}"),
            };

            for sql in [
                raw.sql.clone(),
                match PagedQuery::Raw(raw.clone()).page(10, 20) {
                    Ok((sql, _)) => sql,
                    Err(error) => panic!("{stmt:?} has no page: {error}"),
                },
                match PagedQuery::Raw(raw.clone()).count() {
                    Ok((sql, _)) => sql,
                    Err(error) => panic!("{stmt:?} has no count: {error}"),
                },
            ] {
                assert!(
                    sql.contains(stmt),
                    "{stmt:?} did not survive wrapping into {sql:?}"
                );
            }
        }
    }

    #[test]
    fn a_trailing_line_comment_still_closes() {
        let raw = match wrapped("SELECT $1::int4 AS n -- trailing", 1) {
            Ok(raw) => raw,
            Err(report) => panic!("refused: {report}"),
        };
        assert!(
            raw.sql.contains("-- trailing\n)"),
            "the closing parenthesis is inside the comment: {:?}",
            raw.sql
        );
    }

    #[test]
    fn pages_after_the_caller_s_own_numbering() {
        let cases = [
            ("SELECT 1 AS n", 0),
            ("SELECT $1::int4 AS n", 1),
            ("SELECT $1::int4 + $2::int4 AS n", 2),
        ];

        for (stmt, bound) in cases {
            let raw = match wrapped(stmt, bound) {
                Ok(raw) => raw,
                Err(report) => panic!("{stmt:?} was refused: {report}"),
            };
            let (sql, values) = match PagedQuery::Raw(raw.clone()).page(10, 20) {
                Ok(paged) => paged,
                Err(error) => panic!("{stmt:?} has no page: {error}"),
            };

            assert!(
                sql.ends_with(&format!(" LIMIT ${} OFFSET ${}", bound + 1, bound + 2)),
                "{stmt:?} paged as {sql:?}"
            );
            assert_eq!(
                values.0,
                [
                    vec![Value::Int(Some(7)); bound],
                    vec![Value::BigUnsigned(Some(10)), Value::BigUnsigned(Some(20))],
                ]
                .concat()
            );

            // Counting binds the caller's values and nothing else.
            let (sql, values) = match PagedQuery::Raw(raw).count() {
                Ok(counted) => counted,
                Err(error) => panic!("{stmt:?} has no count: {error}"),
            };
            assert!(!sql.contains("LIMIT"), "{stmt:?} counted as {sql:?}");
            assert_eq!(values.0, vec![Value::Int(Some(7)); bound]);
        }
    }

    #[test]
    fn refuses_a_marker_with_no_value_behind_it() {
        for (stmt, bound, expected) in [
            (
                "SELECT $99::int4 AS n",
                1,
                "reading $99 when 1 bind value was",
            ),
            (
                "SELECT $1::int4 AS n",
                0,
                "reading $1 when 0 bind values were",
            ),
            (
                "SELECT $1::int4 + $3::int4 AS n",
                2,
                "reading $3 when 2 bind values were",
            ),
        ] {
            let report = match wrapped(stmt, bound) {
                Ok(_) => panic!("{stmt:?} was not refused"),
                Err(report) => report,
            };
            assert!(report.contains(expected), "{stmt:?} reported {report:?}");
        }
    }
}
