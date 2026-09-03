//! Schema definition & alternations statements

use crate::{ForeignKeyStatement, IndexStatement, QueryBuilder, TableStatement};

// Boxing a variant would change the public shape of a DDL statement enum callers match on.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum SchemaStatement {
    TableStatement(TableStatement),
    IndexStatement(IndexStatement),
    ForeignKeyStatement(ForeignKeyStatement),
}

// [spec:pgorm:req:sql.ddl+4]
pub trait SchemaStatementBuilder {
    /// Build corresponding SQL statement for certain database backend and return SQL string
    fn build(&self, schema_builder: QueryBuilder) -> String;

    /// Build corresponding SQL statement for certain database backend and return SQL string
    fn build_any(&self, schema_builder: &QueryBuilder) -> String;

    /// Build corresponding SQL statement for certain database backend and return SQL string
    fn to_string(&self, schema_builder: QueryBuilder) -> String {
        self.build(schema_builder)
    }
}
