//! Schema definition & alternations statements

use crate::{ForeignKeyStatement, IndexStatement, TableStatement};

// Boxing a variant would change the public shape of a DDL statement enum callers match on.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum SchemaStatement {
    TableStatement(TableStatement),
    IndexStatement(IndexStatement),
    ForeignKeyStatement(ForeignKeyStatement),
}
