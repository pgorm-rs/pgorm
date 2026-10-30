//! A table's keys and foreign keys, which `CREATE TABLE` declares and
//! `ALTER TABLE` adds alike.

use super::args::{self, ACTIONS, DEFERRABILITY, ENFORCEMENT, choice, columns};
use crate::errors::ConstructionError;
use pgorm::pgorm_query::{ForeignKeyCreateStatement, TableKey, TableName, Unique};
use pyo3::prelude::*;

/// A key of either kind over `columns`: a name, `INCLUDE`d columns, its
/// deferrability and PostgreSQL 18's `WITHOUT OVERLAPS` column, which
/// pgorm-query writes last.
// [spec:pgorm:req:bindings.schema-tables]
pub(super) fn key<K>(
    columns: &Bound<'_, PyAny>,
    name: Option<&Bound<'_, PyAny>>,
    include: Option<&Bound<'_, PyAny>>,
    deferrability: Option<&str>,
    without_overlaps: Option<&Bound<'_, PyAny>>,
) -> PyResult<TableKey<K>> {
    let (first, rest) = self::columns(columns, "a key")?;
    let mut key = TableKey::new(first).cols(rest);
    if let Some(name) = name {
        key = key.name(args::name(name)?);
    }
    if let Some(include) = include {
        key = key.include(args::names(include, "a key's include list")?);
    }
    if let Some(deferrability) = choice(deferrability, "a deferrability", DEFERRABILITY)? {
        key = key.deferrability(deferrability);
    }
    if let Some(period) = without_overlaps {
        key = key.without_overlaps(args::name(period)?);
    }
    Ok(key)
}

/// A unique key, which alone takes `NULLS NOT DISTINCT`.
// [spec:pgorm:req:bindings.schema-tables]
pub(super) fn unique(key: TableKey<Unique>, nulls_not_distinct: bool) -> TableKey<Unique> {
    if nulls_not_distinct {
        key.nulls_not_distinct()
    } else {
        key
    }
}

pub(super) struct ForeignKeyOptions<'a, 'py> {
    pub(super) name: Option<&'a Bound<'py, PyAny>>,
    pub(super) on_delete: Option<&'a str>,
    pub(super) on_update: Option<&'a str>,
    pub(super) deferrability: Option<&'a str>,
    pub(super) enforcement: Option<&'a str>,
    pub(super) period: Option<&'a Bound<'py, PyAny>>,
}

/// A foreign key of `table`'s: its columns paired one to one with the
/// referenced table's, and the options — a name, the referential actions,
/// deferrability, enforcement and PostgreSQL 18's `PERIOD` pair, which
/// pgorm-query writes last on both sides.
// [spec:pgorm:req:bindings.schema-tables]
pub(super) fn foreign_key(
    table: TableName,
    columns: &Bound<'_, PyAny>,
    references: &Bound<'_, PyAny>,
    ref_columns: &Bound<'_, PyAny>,
    options: ForeignKeyOptions<'_, '_>,
) -> PyResult<ForeignKeyCreateStatement> {
    let (first, rest) = self::columns(columns, "a foreign key")?;
    let references = args::relation(references)?;
    let (ref_first, ref_rest) = self::columns(ref_columns, "a foreign key's referenced list")?;
    if rest.len() != ref_rest.len() {
        return Err(ConstructionError::new_err(
            "a foreign key names as many referenced columns as columns, paired in order",
        ));
    }
    let mut key = ForeignKeyCreateStatement::new(table, first, references, ref_first);
    for (column, referenced) in rest.into_iter().zip(ref_rest) {
        key.col(column, referenced);
    }
    if let Some(name) = options.name {
        key.name(args::name(name)?);
    }
    if let Some(action) = choice(options.on_delete, "an ON DELETE action", ACTIONS)? {
        key.on_delete(action);
    }
    if let Some(action) = choice(options.on_update, "an ON UPDATE action", ACTIONS)? {
        key.on_update(action);
    }
    if let Some(deferrability) = choice(options.deferrability, "a deferrability", DEFERRABILITY)? {
        key.deferrability(deferrability);
    }
    if let Some(enforcement) = choice(options.enforcement, "an enforcement", ENFORCEMENT)? {
        key.enforcement(enforcement);
    }
    if let Some(period) = options.period {
        let pair = args::names(period, "a PERIOD pair")?;
        let Ok([column, referenced]) = <[_; 2]>::try_from(pair) else {
            return Err(ConstructionError::new_err(
                "a PERIOD pair is (column, referenced column), one name each side",
            ));
        };
        key.period(column, referenced);
    }
    Ok(key)
}
