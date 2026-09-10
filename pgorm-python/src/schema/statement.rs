use crate::expressions::Compiled;
use pgorm::pgorm_query::{
    ColumnRenameStatement, CommentStatement, IndexDropStatement, TableAlterStatement,
    TableDropStatement, TableRenameStatement, TableTruncateStatement, Values,
    extension::{TypeAlterStatement, TypeCreateStatement, TypeDropStatement},
};
use pyo3::prelude::*;

/// Owned native DDL states; no textual reconstruction or deferred Python calls.
#[derive(Clone, Debug)]
pub(crate) enum Statement {
    AlterTable(TableAlterStatement),
    DropTable(TableDropStatement),
    RenameTable(TableRenameStatement),
    RenameColumn(ColumnRenameStatement),
    Truncate(TableTruncateStatement),
    DropIndex(IndexDropStatement),
    CreateEnum(TypeCreateStatement),
    AlterEnum(TypeAlterStatement),
    DropEnum(TypeDropStatement),
    Comment(CommentStatement),
}

#[derive(Clone, Debug)]
#[pyclass(name = "DDL", module = "pgorm.schema", frozen, from_py_object)]
pub struct PyDDL {
    pub(crate) inner: Statement,
}

#[pymethods]
impl PyDDL {
    // [spec:pgorm:req:python.schema]
    pub fn inspect(&self) -> Compiled {
        let sql = match &self.inner {
            Statement::AlterTable(s) => s.to_string(),
            Statement::DropTable(s) => s.to_string(),
            Statement::RenameTable(s) => s.to_string(),
            Statement::RenameColumn(s) => s.to_string(),
            Statement::Truncate(s) => s.to_string(),
            Statement::DropIndex(s) => s.to_string(),
            Statement::CreateEnum(s) => s.to_string(),
            Statement::AlterEnum(s) => s.to_string(),
            Statement::DropEnum(s) => s.to_string(),
            Statement::Comment(s) => s.to_string(),
        };
        Compiled {
            sql,
            values: Values(Vec::new()),
        }
    }
}
