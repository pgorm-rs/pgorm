//! Comment statements.
//!
//! PostgreSQL attaches a comment to a schema object with a standalone
//! `COMMENT ON` statement rather than a clause of `CREATE TABLE`, so comments
//! are built here and executed alongside the DDL that creates the object they
//! describe.
//!
//! # Usage
//!
//! - Table comment, see [`Comment::on_table`]
//! - Column comment, see [`Comment::on_column`]

use inherent::inherent;

use crate::{DynIden, IntoIden, QueryBuilder, SchemaStatementBuilder, TableRef};

/// Helper for constructing any comment statement
// [spec:pgorm:req:sql.ddl+2]
// [spec:pgorm:req:sql.ddl.comment]
#[derive(Debug)]
pub struct Comment;

/// The name of a table a comment can be attached to.
///
/// This is the subset of [`TableRef`] that names a table: the alias-carrying,
/// subquery, values-list and function-call forms have no object to comment on,
/// so they are not representable here.
#[derive(Debug, Clone, PartialEq)]
pub enum CommentTable {
    /// Table identifier without any schema / database prefix
    Table(DynIden),
    /// Table identifier with schema prefix
    SchemaTable(DynIden, DynIden),
    /// Table identifier with database and schema prefix
    DatabaseSchemaTable(DynIden, DynIden, DynIden),
}

/// Conversion into the table name a comment targets.
pub trait IntoCommentTable {
    /// Consume `self` and produce a [`CommentTable`]
    fn into_comment_table(self) -> CommentTable;
}

/// The object a [`CommentStatement`] is attached to.
#[derive(Debug, Clone, PartialEq)]
pub enum CommentTarget {
    /// A whole table
    Table(CommentTable),
    /// A single column of a table
    Column(CommentTable, DynIden),
}

/// A [`TableRef`] that names no table — a subquery, a values list or a
/// function call — and so cannot carry a comment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnnamedTableRef;

impl std::fmt::Display for UnnamedTableRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "table reference does not name a table")
    }
}

impl std::error::Error for UnnamedTableRef {}

/// Attach a comment to a table or one of its columns
///
/// # Examples
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// assert_eq!(
///     Comment::on_table(Char::Table, "one row per character").to_string(QueryBuilder),
///     r#"COMMENT ON TABLE "character" IS 'one row per character'"#
/// );
///
/// assert_eq!(
///     Comment::on_column(Char::Table, Char::FontSize, "in points").to_string(QueryBuilder),
///     r#"COMMENT ON COLUMN "character"."font_size" IS 'in points'"#
/// );
/// ```
///
/// The target may be schema-qualified, and comment text is written as a
/// standard-conforming string literal:
///
/// ```
/// use pgorm_query::{*, tests_cfg::*};
///
/// assert_eq!(
///     Comment::on_table((Alias::new("public"), Char::Table), "it's a table")
///         .to_string(QueryBuilder),
///     r#"COMMENT ON TABLE "public"."character" IS 'it''s a table'"#
/// );
/// ```
// [spec:pgorm:req:sql.ddl.comment]
#[derive(Debug, Clone, PartialEq)]
pub struct CommentStatement {
    pub(crate) target: CommentTarget,
    pub(crate) comment: String,
}

impl Comment {
    /// Construct a `COMMENT ON TABLE` statement
    pub fn on_table<T, C>(table: T, comment: C) -> CommentStatement
    where
        T: IntoCommentTable,
        C: Into<String>,
    {
        CommentStatement {
            target: CommentTarget::Table(table.into_comment_table()),
            comment: comment.into(),
        }
    }

    /// Construct a `COMMENT ON COLUMN` statement
    pub fn on_column<T, N, C>(table: T, column: N, comment: C) -> CommentStatement
    where
        T: IntoCommentTable,
        N: IntoIden,
        C: Into<String>,
    {
        CommentStatement {
            target: CommentTarget::Column(table.into_comment_table(), column.into_iden()),
            comment: comment.into(),
        }
    }
}

impl CommentStatement {
    /// Get the object this comment is attached to
    pub fn get_target(&self) -> &CommentTarget {
        &self.target
    }

    /// Get the comment text, unescaped
    pub fn get_comment(&self) -> &str {
        &self.comment
    }
}

impl IntoCommentTable for CommentTable {
    fn into_comment_table(self) -> CommentTable {
        self
    }
}

impl<T: 'static> IntoCommentTable for T
where
    T: IntoIden,
{
    fn into_comment_table(self) -> CommentTable {
        CommentTable::Table(self.into_iden())
    }
}

impl<S: 'static, T: 'static> IntoCommentTable for (S, T)
where
    S: IntoIden,
    T: IntoIden,
{
    fn into_comment_table(self) -> CommentTable {
        CommentTable::SchemaTable(self.0.into_iden(), self.1.into_iden())
    }
}

impl<D: 'static, S: 'static, T: 'static> IntoCommentTable for (D, S, T)
where
    D: IntoIden,
    S: IntoIden,
    T: IntoIden,
{
    fn into_comment_table(self) -> CommentTable {
        CommentTable::DatabaseSchemaTable(
            self.0.into_iden(),
            self.1.into_iden(),
            self.2.into_iden(),
        )
    }
}

/// Take the table a [`TableRef`] names, dropping any alias.
// [spec:pgorm:req:sql.ddl.comment]
impl TryFrom<TableRef> for CommentTable {
    type Error = UnnamedTableRef;

    fn try_from(table_ref: TableRef) -> Result<Self, Self::Error> {
        match table_ref {
            TableRef::Table(table) | TableRef::TableAlias(table, _) => Ok(Self::Table(table)),
            TableRef::SchemaTable(schema, table) | TableRef::SchemaTableAlias(schema, table, _) => {
                Ok(Self::SchemaTable(schema, table))
            }
            TableRef::DatabaseSchemaTable(database, schema, table)
            | TableRef::DatabaseSchemaTableAlias(database, schema, table, _) => {
                Ok(Self::DatabaseSchemaTable(database, schema, table))
            }
            TableRef::SubQuery(_, _)
            | TableRef::ValuesList(_, _)
            | TableRef::FunctionCall(_, _) => Err(UnnamedTableRef),
        }
    }
}

// [spec:pgorm:req:sql.ddl+2]
#[inherent]
impl SchemaStatementBuilder for CommentStatement {
    pub fn build(&self, schema_builder: QueryBuilder) -> String {
        let mut sql = String::with_capacity(128);
        schema_builder.prepare_comment_statement(self, &mut sql);
        sql
    }

    pub fn build_any(&self, schema_builder: &QueryBuilder) -> String {
        let mut sql = String::with_capacity(128);
        schema_builder.prepare_comment_statement(self, &mut sql);
        sql
    }

    pub fn to_string(&self, schema_builder: QueryBuilder) -> String;
}
