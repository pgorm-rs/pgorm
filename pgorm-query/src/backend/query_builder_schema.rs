//! `CREATE SCHEMA`, `DROP SCHEMA` and `ALTER SCHEMA ... RENAME TO`: one
//! unqualified name each, quoted.

use super::*;

impl QueryBuilder {
    // [spec:pgorm:req:sql.ddl.schema]
    pub(crate) fn prepare_schema_create_statement(
        &self,
        create: &SchemaCreateStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "CREATE SCHEMA ").unwrap();
        if create.if_not_exists {
            write!(sql, "IF NOT EXISTS ").unwrap();
        }
        create.name.prepare(sql.as_writer());
    }

    // [spec:pgorm:req:sql.ddl.schema]
    pub(crate) fn prepare_schema_drop_statement(
        &self,
        drop: &SchemaDropStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "DROP SCHEMA ").unwrap();
        if drop.if_exists {
            write!(sql, "IF EXISTS ").unwrap();
        }
        drop.names().fold(true, |first, name| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            name.prepare(sql.as_writer());
            false
        });
        if let Some(behavior) = drop.behavior {
            write!(sql, " {}", behavior.keyword()).unwrap();
        }
    }

    // [spec:pgorm:req:sql.ddl.schema]
    pub(crate) fn prepare_schema_rename_statement(
        &self,
        rename: &SchemaRenameStatement,
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "ALTER SCHEMA ").unwrap();
        rename.from_name.prepare(sql.as_writer());
        write!(sql, " RENAME TO ").unwrap();
        rename.to_name.prepare(sql.as_writer());
    }
}
