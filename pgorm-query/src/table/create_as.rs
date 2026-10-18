use crate::{IntoName, Name, QueryBuilder, SelectStatement, TableName};

/// Create a table from a query: `CREATE TABLE [IF NOT EXISTS ]"t"[ ("c",
/// ...)] AS <query>[ WITH NO DATA]`.
///
/// The table takes its columns' names and types from the query's, the names
/// given here renaming the leading ones, and its rows from the query's rows
/// when the statement runs, or none under
/// [`with_no_data`](Self::with_no_data). It is a table like any other from
/// then on: unlike a materialized view, nothing ties it to the query again.
/// The query's values are written into the statement as literals, which is
/// how every DDL statement carries a value.
///
/// The name and the query are taken by [`Table::create_as`](crate::Table::create_as).
///
/// # Examples
///
/// ```
/// use pgorm_query::*;
///
/// let create = Table::create_as(
///     Name::runtime("archived_order"),
///     Query::select()
///         .columns([Name::runtime("id"), Name::runtime("total")])
///         .from(Name::runtime("order"))
///         .and_where(Expr::col(Name::runtime("placed")).lt("2020-01-01"))
///         .take(),
/// )
/// .if_not_exists()
/// .column(Name::runtime("order_id"))
/// .to_owned();
///
/// assert_eq!(
///     create.to_string(),
///     [
///         r#"CREATE TABLE IF NOT EXISTS "archived_order" ("order_id") AS"#,
///         r#"SELECT "id", "total" FROM "order" WHERE "placed" < '2020-01-01'"#,
///     ]
///     .join(" ")
/// );
/// ```
// [spec:pgorm:req:sql.ddl.create-table-as]
#[derive(Debug, Clone)]
pub struct TableCreateAsStatement {
    pub(crate) name: TableName,
    pub(crate) columns: Vec<Name>,
    pub(crate) body: SelectStatement,
    pub(crate) if_not_exists: bool,
    pub(crate) no_data: bool,
}

impl TableCreateAsStatement {
    /// `IF NOT EXISTS`: leave an existing relation of this name alone, its
    /// rows with it.
    pub fn if_not_exists(&mut self) -> &mut Self {
        self.if_not_exists = true;
        self
    }

    /// Name the next column of the table.
    pub fn column<C>(&mut self, column: C) -> &mut Self
    where
        C: IntoName,
    {
        self.columns.push(column.into_name());
        self
    }

    /// Name the next columns of the table, in order.
    pub fn columns<I, C>(&mut self, columns: I) -> &mut Self
    where
        I: IntoIterator<Item = C>,
        C: IntoName,
    {
        self.columns
            .extend(columns.into_iter().map(IntoName::into_name));
        self
    }

    /// `WITH NO DATA`: create the table with the query's columns and none of
    /// its rows.
    pub fn with_no_data(&mut self) -> &mut Self {
        self.no_data = true;
        self
    }

    /// The column names given, in order.
    pub fn get_columns(&self) -> &[Name] {
        &self.columns
    }
}

/// Renders the statement with every value inlined as an SQL literal. This is
/// its only rendering: a DDL statement takes no bind parameters, so the values
/// its query carries are written into it as escaped literals.
// [spec:pgorm:req:sql.ddl+9]
impl std::fmt::Display for TableCreateAsStatement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut sql = String::with_capacity(256);
        QueryBuilder.prepare_table_create_as_statement(self, &mut sql);
        f.write_str(&sql)
    }
}
