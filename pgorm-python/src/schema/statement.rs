use crate::{errors::ConstructionError, expressions::Compiled};
use pgorm::pgorm_query::{
    ColumnRenameStatement, CommentStatement, ConstraintRenameStatement, DomainAlterStatement,
    DomainDropStatement, IndexDropStatement, MaterializedViewCreateStatement,
    MaterializedViewDropStatement, MaterializedViewRefreshStatement, RecursiveViewCreateStatement,
    SchemaCreateStatement, SchemaDropStatement, SchemaRenameStatement, TableAlterStatement,
    TableCreateAsStatement, TableDropStatement, TableRenameStatement, TableTruncateStatement,
    Values, ViewCreateStatement, ViewDropStatement,
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
    RenameConstraint(ConstraintRenameStatement),
    Truncate(TableTruncateStatement),
    DropIndex(IndexDropStatement),
    CreateEnum(TypeCreateStatement),
    AlterEnum(TypeAlterStatement),
    DropEnum(TypeDropStatement),
    Comment(CommentStatement),
    CreateView(ViewCreateStatement),
    CreateRecursiveView(RecursiveViewCreateStatement),
    DropView(ViewDropStatement),
    CreateMaterializedView(MaterializedViewCreateStatement),
    RefreshMaterializedView(MaterializedViewRefreshStatement),
    DropMaterializedView(MaterializedViewDropStatement),
    CreateTableAs(TableCreateAsStatement),
    CreateSchema(SchemaCreateStatement),
    DropSchema(SchemaDropStatement),
    RenameSchema(SchemaRenameStatement),
    AlterDomain(DomainAlterStatement),
    DropDomain(DomainDropStatement),
}

#[derive(Clone, Debug)]
#[pyclass(name = "DDL", module = "pgorm.schema", frozen, from_py_object)]
pub struct PyDDL {
    pub(crate) inner: Statement,
}

#[pymethods]
impl PyDDL {
    // [spec:pgorm:req:bindings.schema]
    pub fn inspect(&self) -> Compiled {
        let sql = match &self.inner {
            Statement::AlterTable(s) => s.to_string(),
            Statement::DropTable(s) => s.to_string(),
            Statement::RenameTable(s) => s.to_string(),
            Statement::RenameColumn(s) => s.to_string(),
            Statement::RenameConstraint(s) => s.to_string(),
            Statement::Truncate(s) => s.to_string(),
            Statement::DropIndex(s) => s.to_string(),
            Statement::CreateEnum(s) => s.to_string(),
            Statement::AlterEnum(s) => s.to_string(),
            Statement::DropEnum(s) => s.to_string(),
            Statement::Comment(s) => s.to_string(),
            Statement::CreateView(s) => s.to_string(),
            Statement::CreateRecursiveView(s) => s.to_string(),
            Statement::DropView(s) => s.to_string(),
            Statement::CreateMaterializedView(s) => s.to_string(),
            Statement::RefreshMaterializedView(s) => s.to_string(),
            Statement::DropMaterializedView(s) => s.to_string(),
            Statement::CreateTableAs(s) => s.to_string(),
            Statement::CreateSchema(s) => s.to_string(),
            Statement::DropSchema(s) => s.to_string(),
            Statement::RenameSchema(s) => s.to_string(),
            Statement::AlterDomain(s) => s.to_string(),
            Statement::DropDomain(s) => s.to_string(),
        };
        Compiled {
            sql,
            values: Values(Vec::new()),
        }
    }

    fn __bool__(&self) -> PyResult<bool> {
        Err(ConstructionError::new_err(
            "SQL statements cannot be tested as Python booleans",
        ))
    }
}
