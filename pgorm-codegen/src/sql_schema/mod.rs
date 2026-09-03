//! Entities from DDL text, with no database to inspect.
//!
//! [`parse_schema`] reads a `schema.sql` with libpg_query — the PostgreSQL
//! server's own parser — and bridges the parse tree into the
//! [`TableCreateStatement`]s [`EntityTransformer::transform`] consumes;
//! [`entities_from_sql`] runs the whole pipeline and hands back the generated
//! files.
//!
//! The bridge understands the DDL the entity model has a place for — tables,
//! columns, primary keys, unique constraints, foreign keys, enum types,
//! indexes and comments. Anything else in the file is a named error rather
//! than a silent omission: a schema that generates entities is a schema the
//! bridge understood in full.
//!
//! ```
//! use pgorm_codegen::{EntityWriterOptions, sql_schema};
//!
//! let files = sql_schema::entities_from_sql(
//!     "CREATE TABLE cake (id serial PRIMARY KEY, name text NOT NULL);",
//!     EntityWriterOptions::default(),
//! )
//! .expect("a supported schema");
//!
//! assert!(files.files.iter().any(|file| file.name == "cake.rs"));
//! ```
#![deny(
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic,
    clippy::unwrap_used
)]

mod objects;
mod table;
mod types;

use crate::{EntityTransformer, EntityWriterContext, EntityWriterOptions, Error, WriterOutput};
use pg_query::NodeEnum;
use pg_query::protobuf::{CommentStmt, CreateStmt, IndexStmt};
use pgorm_query::TableCreateStatement;
use std::collections::BTreeMap;
use std::fmt::Display;

/// Enum type name → its values, in declaration order.
type Enums = BTreeMap<String, Vec<String>>;

/// Read DDL text as the schema statements the entity transformer consumes.
///
/// Enum types are resolved into the column types that name them, and indexes
/// and comments are folded into the table they describe, so the returned
/// statements stand alone.
// [spec:pgorm:def:codegen.ddl+1]
// [spec:pgorm:req:codegen.ddl.unsupported]
pub fn parse_schema(sql: &str) -> Result<Vec<TableCreateStatement>, Error> {
    let parsed = pg_query::parse(sql)
        .map_err(|err| Error::TransformError(format!("schema SQL did not parse: {err}")))?;
    build(collect(&parsed.protobuf)?)
}

/// Generate entity files straight from DDL text.
///
/// The whole pipeline: parse, bridge, [`EntityTransformer::transform`],
/// [`crate::EntityWriter::generate`]. Options are validated first, so an
/// unusable option is reported before the schema is read.
// [spec:pgorm:def:codegen.ddl+1]
pub fn entities_from_sql(sql: &str, options: EntityWriterOptions) -> Result<WriterOutput, Error> {
    let context = EntityWriterContext::new(options)?;
    let writer = EntityTransformer::transform(parse_schema(sql)?)?;
    Ok(writer.generate(&context))
}

/// A construct the bridge does not carry into the entity model.
// [spec:pgorm:req:codegen.ddl.unsupported]
fn unsupported(what: impl Display, at: usize) -> Error {
    Error::TransformError(format!("unsupported DDL: {what} at statement {at}"))
}

/// A construct the bridge understands but cannot resolve in this schema.
// [spec:pgorm:req:codegen.ddl.unsupported]
fn unresolved(problem: impl Display, at: usize) -> Error {
    Error::TransformError(format!("statement {at}: {problem}"))
}

/// The statements of one parse, sorted by kind and paired with their 1-based
/// position in the file.
#[derive(Default)]
struct Collected<'a> {
    enums: Enums,
    tables: Vec<(usize, &'a CreateStmt)>,
    indexes: Vec<(usize, &'a IndexStmt)>,
    comments: Vec<(usize, &'a CommentStmt)>,
}

/// Sort every statement in the file into the four the bridge reads, refusing
/// anything else by name.
// [spec:pgorm:req:codegen.ddl.unsupported]
fn collect(parsed: &pg_query::protobuf::ParseResult) -> Result<Collected<'_>, Error> {
    let mut collected = Collected::default();
    for (index, raw) in parsed.stmts.iter().enumerate() {
        let at = index + 1;
        let Some(node) = raw.stmt.as_ref().and_then(|stmt| stmt.node.as_ref()) else {
            return Err(unsupported("an empty statement", at));
        };
        match node {
            NodeEnum::CreateStmt(stmt) => collected.tables.push((at, stmt)),
            NodeEnum::IndexStmt(stmt) => collected.indexes.push((at, stmt.as_ref())),
            NodeEnum::CommentStmt(stmt) => collected.comments.push((at, stmt.as_ref())),
            NodeEnum::CreateEnumStmt(stmt) => {
                let (name, values) = objects::enum_type(stmt, at)?;
                if collected.enums.insert(name.clone(), values).is_some() {
                    return Err(unresolved(format!("type `{name}` is declared twice"), at));
                }
            }
            other => return Err(unsupported(statement_kind(other), at)),
        }
    }
    Ok(collected)
}

/// Resolve the collected statements against each other: enum types into the
/// columns naming them, indexes and comments into the table they describe.
// [spec:pgorm:sem:codegen.ddl.objects]
fn build(collected: Collected<'_>) -> Result<Vec<TableCreateStatement>, Error> {
    let Collected {
        enums,
        tables,
        indexes,
        comments,
    } = collected;

    let mut positions: BTreeMap<&str, usize> = BTreeMap::new();
    for (position, (at, stmt)) in tables.iter().enumerate() {
        let name = table::name(stmt, *at)?;
        if positions.insert(table::relname(stmt), position).is_some() {
            return Err(unresolved(format!("table `{name}` is declared twice"), *at));
        }
    }

    let mut attachments: Vec<table::Attachments> = Vec::new();
    attachments.resize_with(tables.len(), table::Attachments::default);
    let position_of = |table: &str, at: usize| -> Result<usize, Error> {
        positions
            .get(table)
            .copied()
            .ok_or_else(|| unresolved(format!("no CREATE TABLE for table `{table}`"), at))
    };

    for (at, stmt) in indexes {
        let parsed = objects::index(stmt, at)?;
        let position = position_of(&parsed.table, at)?;
        if let (Some(slot), Some(index)) = (attachments.get_mut(position), parsed.index) {
            slot.indexes.push(index);
        }
    }
    for (at, stmt) in comments {
        let parsed = objects::comment(stmt, at)?;
        let position = position_of(parsed.table(), at)?;
        let Some(slot) = attachments.get_mut(position) else {
            continue;
        };
        match parsed {
            objects::ParsedComment::Table { text, .. } => slot.table_comment = Some(text),
            objects::ParsedComment::Column { column, text, .. } => {
                slot.column_comments.insert(column, (at, text));
            }
        }
    }

    let mut statements = Vec::with_capacity(tables.len());
    for (position, (at, stmt)) in tables.into_iter().enumerate() {
        let attachment = attachments
            .get_mut(position)
            .map(std::mem::take)
            .unwrap_or_default();
        statements.push(table::build(stmt, at, &enums, attachment)?);
    }
    Ok(statements)
}

/// The SQL a statement the bridge does not read was written as, named the way
/// its author wrote it.
// [spec:pgorm:req:codegen.ddl.unsupported]
fn statement_kind(node: &NodeEnum) -> &'static str {
    match node {
        NodeEnum::AlterTableStmt(_) => "ALTER TABLE",
        NodeEnum::AlterSeqStmt(_) => "ALTER SEQUENCE",
        NodeEnum::AlterTypeStmt(_) => "ALTER TYPE",
        NodeEnum::AlterOwnerStmt(_) => "ALTER ... OWNER TO",
        NodeEnum::AlterObjectSchemaStmt(_) => "ALTER ... SET SCHEMA",
        NodeEnum::RenameStmt(_) => "ALTER ... RENAME",
        NodeEnum::CreateSchemaStmt(_) => "CREATE SCHEMA",
        NodeEnum::CreateSeqStmt(_) => "CREATE SEQUENCE",
        NodeEnum::CreateTrigStmt(_) => "CREATE TRIGGER",
        NodeEnum::CreateFunctionStmt(_) => "CREATE FUNCTION",
        NodeEnum::CreateDomainStmt(_) => "CREATE DOMAIN",
        NodeEnum::CreateExtensionStmt(_) => "CREATE EXTENSION",
        NodeEnum::CreatePolicyStmt(_) => "CREATE POLICY",
        NodeEnum::CreateForeignTableStmt(_) => "CREATE FOREIGN TABLE",
        NodeEnum::CreateTableAsStmt(_) => "CREATE TABLE AS",
        NodeEnum::CreateStatsStmt(_) => "CREATE STATISTICS",
        NodeEnum::CreateRangeStmt(_) => "CREATE TYPE ... AS RANGE",
        NodeEnum::CompositeTypeStmt(_) => "CREATE TYPE ... AS",
        NodeEnum::ViewStmt(_) => "CREATE VIEW",
        NodeEnum::RuleStmt(_) => "CREATE RULE",
        NodeEnum::DefineStmt(_) => "CREATE TYPE, OPERATOR or AGGREGATE",
        NodeEnum::GrantStmt(_) => "GRANT or REVOKE",
        NodeEnum::DropStmt(_) => "DROP",
        NodeEnum::TruncateStmt(_) => "TRUNCATE",
        NodeEnum::InsertStmt(_) => "INSERT",
        NodeEnum::UpdateStmt(_) => "UPDATE",
        NodeEnum::DeleteStmt(_) => "DELETE",
        NodeEnum::SelectStmt(_) => "SELECT",
        NodeEnum::CopyStmt(_) => "COPY",
        NodeEnum::DoStmt(_) => "DO",
        NodeEnum::VariableSetStmt(_) => "SET",
        NodeEnum::TransactionStmt(_) => "a transaction statement",
        _ => "an unrecognised statement",
    }
}
