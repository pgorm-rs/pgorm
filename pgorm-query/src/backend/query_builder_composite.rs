//! `CREATE TYPE ... AS (composite)`'s attribute list.

use super::*;
use crate::extension::CompositeAttribute;

impl QueryBuilder {
    /// A composite's `(<attribute>, ...)`, each attribute its quoted name, the
    /// type as a column writes it, and its collation when it has one. The
    /// parentheses are written for an empty list too: `AS ()` is the empty
    /// composite, and `AS` alone is no statement.
    // [spec:pgorm:req:sql.ddl.type-composite]
    pub(super) fn prepare_composite_attributes(
        &self,
        attributes: &[CompositeAttribute],
        sql: &mut dyn SqlWriter,
    ) {
        write!(sql, "(").unwrap();
        attributes.iter().fold(true, |first, attribute| {
            if !first {
                write!(sql, ", ").unwrap();
            }
            attribute.name.prepare(sql.as_writer());
            write!(sql, " ").unwrap();
            self.prepare_column_type(&attribute.column_type, sql);
            if let Some(collation) = &attribute.collation {
                write!(sql, " COLLATE ").unwrap();
                collation.prepare(sql.as_writer());
            }
            false
        });
        write!(sql, ")").unwrap();
    }
}
