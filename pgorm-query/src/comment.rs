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

use crate::{DynIden, IntoIden, IntoTableName, QueryBuilder, SchemaStatementBuilder, TableName};

/// Helper for constructing any comment statement
// [spec:pgorm:req:sql.ddl+3]
// [spec:pgorm:req:sql.ddl.comment+1]
#[derive(Debug)]
pub struct Comment;

/// The object a [`CommentStatement`] is attached to.
#[derive(Debug, Clone, PartialEq)]
pub enum CommentTarget {
    /// A whole table
    Table(TableName),
    /// A single column of a table
    Column(TableName, DynIden),
}

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
// [spec:pgorm:req:sql.ddl.comment+1]
#[derive(Debug, Clone, PartialEq)]
pub struct CommentStatement {
    pub(crate) target: CommentTarget,
    pub(crate) comment: String,
}

impl Comment {
    /// Construct a `COMMENT ON TABLE` statement
    pub fn on_table<T, C>(table: T, comment: C) -> CommentStatement
    where
        T: IntoTableName,
        C: Into<String>,
    {
        CommentStatement {
            target: CommentTarget::Table(table.into_table_name()),
            comment: comment.into(),
        }
    }

    /// Construct a `COMMENT ON COLUMN` statement
    pub fn on_column<T, N, C>(table: T, column: N, comment: C) -> CommentStatement
    where
        T: IntoTableName,
        N: IntoIden,
        C: Into<String>,
    {
        CommentStatement {
            target: CommentTarget::Column(table.into_table_name(), column.into_iden()),
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

// [spec:pgorm:req:sql.ddl+3]
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
