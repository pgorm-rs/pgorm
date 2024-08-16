use sea_query::{inject_parameters, PostgresQueryBuilder};
pub use sea_query::{Value, Values};
use std::fmt;

/// Defines an SQL statement
#[derive(Debug, Clone, PartialEq)]
pub struct Statement {
    /// The SQL query
    pub sql: String,
    /// The values for the SQL statement's parameters
    pub values: Option<Values>,
}

/// Any type that can build a [Statement]
pub trait StatementBuilder {
    /// Method to call in order to build a [Statement]
    fn build(&self) -> Statement;
}

impl Statement {
    /// Create a [Statement] from a [crate::DatabaseBackend] and a raw SQL statement
    pub fn from_string<T>(stmt: T) -> Statement
    where
        T: Into<String>,
    {
        Statement {
            sql: stmt.into(),
            values: None,
        }
    }

    /// Create a SQL statement from a [crate::DatabaseBackend], a
    /// raw SQL statement and param values
    pub fn from_sql_and_values<I, T>(sql: T, values: I) -> Self
    where
        I: IntoIterator<Item = Value>,
        T: Into<String>,
    {
        Self::from_string_values_tuple((sql, Values(values.into_iter().collect())))
    }

    pub(crate) fn from_string_values_tuple<T>(stmt: (T, Values)) -> Statement
    where
        T: Into<String>,
    {
        Statement {
            sql: stmt.0.into(),
            values: Some(stmt.1),
        }
    }
}

impl fmt::Display for Statement {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.values {
            Some(values) => {
                let string = inject_parameters(
                    &self.sql,
                    values.0.clone(),
                    &sea_query::PostgresQueryBuilder,
                );
                write!(f, "{}", &string)
            }
            None => {
                write!(f, "{}", &self.sql)
            }
        }
    }
}

macro_rules! build_any_stmt {
    ($stmt: expr) => {
        $stmt.build(sea_query::PostgresQueryBuilder)
    };
}

macro_rules! build_postgres_stmt {
    ($stmt: expr) => {
        $stmt.to_string(sea_query::PostgresQueryBuilder)
    };
}

macro_rules! build_query_stmt {
    ($stmt: ty) => {
        impl StatementBuilder for $stmt {
            fn build(&self) -> Statement {
                let stmt = self.build(sea_query::PostgresQueryBuilder);
                Statement::from_string_values_tuple(stmt)
            }
        }
    };
}

build_query_stmt!(sea_query::InsertStatement);
build_query_stmt!(sea_query::SelectStatement);
build_query_stmt!(sea_query::UpdateStatement);
build_query_stmt!(sea_query::DeleteStatement);
build_query_stmt!(sea_query::WithQuery);

macro_rules! build_schema_stmt {
    ($stmt: ty) => {
        impl StatementBuilder for $stmt {
            fn build(&self) -> Statement {
                let stmt = build_any_stmt!(self);
                Statement::from_string(stmt)
            }
        }
    };
}

build_schema_stmt!(sea_query::TableCreateStatement);
build_schema_stmt!(sea_query::TableDropStatement);
build_schema_stmt!(sea_query::TableAlterStatement);
build_schema_stmt!(sea_query::TableRenameStatement);
build_schema_stmt!(sea_query::TableTruncateStatement);
build_schema_stmt!(sea_query::IndexCreateStatement);
build_schema_stmt!(sea_query::IndexDropStatement);
build_schema_stmt!(sea_query::ForeignKeyCreateStatement);
build_schema_stmt!(sea_query::ForeignKeyDropStatement);

macro_rules! build_type_stmt {
    ($stmt: ty) => {
        impl StatementBuilder for $stmt {
            fn build(&self) -> Statement {
                let stmt = build_postgres_stmt!(self);
                Statement::from_string(stmt)
            }
        }
    };
}

build_type_stmt!(sea_query::extension::postgres::TypeAlterStatement);
build_type_stmt!(sea_query::extension::postgres::TypeCreateStatement);
build_type_stmt!(sea_query::extension::postgres::TypeDropStatement);
