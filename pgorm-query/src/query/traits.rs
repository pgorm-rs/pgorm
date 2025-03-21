use std::fmt::Debug;

use crate::{SqlWriter, SqlWriterValues, SubQueryStatement, backend::QueryBuilder, value::Values};

pub trait QueryStatementBuilder: Debug {
    /// Build corresponding SQL statement for certain database backend and collect query parameters into a vector
    fn build_any(&self, query_builder: &QueryBuilder) -> (String, Values) {
        let (placeholder, numbered) = query_builder.placeholder();
        let mut sql = SqlWriterValues::new(placeholder, numbered);
        self.build_collect_any_into(query_builder, &mut sql);
        sql.into_parts()
    }

    /// Build corresponding SQL statement for certain database backend and collect query parameters
    fn build_collect_any(&self, query_builder: &QueryBuilder, sql: &mut dyn SqlWriter) -> String {
        self.build_collect_any_into(query_builder, sql);
        sql.to_string()
    }

    /// Build corresponding SQL statement into the SqlWriter for certain database backend and collect query parameters
    fn build_collect_any_into(&self, query_builder: &QueryBuilder, sql: &mut dyn SqlWriter);

    fn into_sub_query_statement(self) -> SubQueryStatement;
}

pub trait QueryStatementWriter: QueryStatementBuilder {
    /// Build corresponding SQL statement for certain database backend and return SQL string
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .column(Glyph::Aspect)
    ///     .from(Glyph::Table)
    ///     .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
    ///     .order_by(Glyph::Image, Order::Desc)
    ///     .order_by((Glyph::Table, Glyph::Aspect), Order::Asc)
    ///     .to_string(QueryBuilder);
    ///
    /// assert_eq!(
    ///     query,
    ///     r#"SELECT "aspect" FROM "glyph" WHERE COALESCE("aspect", 0) > 2 ORDER BY "image" DESC, "glyph"."aspect" ASC"#
    /// );
    /// ```
    fn to_string(&self, query_builder: QueryBuilder) -> String {
        let mut sql = String::with_capacity(256);
        self.build_collect_any_into(&query_builder, &mut sql);
        sql
    }

    /// Build corresponding SQL statement for certain database backend and collect query parameters into a vector
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let (query, params) = Query::select()
    ///     .column(Glyph::Aspect)
    ///     .from(Glyph::Table)
    ///     .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
    ///     .order_by(Glyph::Image, Order::Desc)
    ///     .order_by((Glyph::Table, Glyph::Aspect), Order::Asc)
    ///     .build(QueryBuilder);
    ///
    /// assert_eq!(
    ///     query,
    ///     r#"SELECT "aspect" FROM "glyph" WHERE COALESCE("aspect", $1) > $2 ORDER BY "image" DESC, "glyph"."aspect" ASC"#
    /// );
    /// assert_eq!(
    ///     params,
    ///     Values(vec![Value::Int(Some(0)), Value::Int(Some(2))])
    /// );
    /// ```
    fn build(&self, query_builder: QueryBuilder) -> (String, Values) {
        let (placeholder, numbered) = query_builder.placeholder();
        let mut sql = SqlWriterValues::new(placeholder, numbered);
        self.build_collect_into(query_builder, &mut sql);
        sql.into_parts()
    }

    /// Build corresponding SQL statement for certain database backend and collect query parameters
    ///
    /// # Examples
    ///
    /// ```
    /// use pgorm_query::{*, tests_cfg::*};
    ///
    /// let query = Query::select()
    ///     .column(Glyph::Aspect)
    ///     .from(Glyph::Table)
    ///     .and_where(Expr::expr(Expr::col(Glyph::Aspect).if_null(0)).gt(2))
    ///     .order_by(Glyph::Image, Order::Desc)
    ///     .order_by((Glyph::Table, Glyph::Aspect), Order::Asc)
    ///     .to_owned();
    ///
    /// let (placeholder, numbered) = QueryBuilder.placeholder();
    /// let mut sql = SqlWriterValues::new(placeholder, numbered);
    ///
    /// assert_eq!(
    ///     query.build_collect(QueryBuilder, &mut sql),
    ///     r#"SELECT "aspect" FROM "glyph" WHERE COALESCE("aspect", $1) > $2 ORDER BY "image" DESC, "glyph"."aspect" ASC"#
    /// );
    ///
    /// let (sql, values) = sql.into_parts();
    /// assert_eq!(
    ///     values,
    ///     Values(vec![Value::Int(Some(0)), Value::Int(Some(2))])
    /// );
    /// ```
    fn build_collect(&self, query_builder: QueryBuilder, sql: &mut dyn SqlWriter) -> String {
        self.build_collect_into(query_builder, sql);
        sql.to_string()
    }

    fn build_collect_into(&self, query_builder: QueryBuilder, sql: &mut dyn SqlWriter);
}
