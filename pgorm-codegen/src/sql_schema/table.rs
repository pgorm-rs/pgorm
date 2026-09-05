use super::{Enums, types, unresolved, unsupported};
use crate::Error;
use pg_query::NodeEnum;
use pg_query::protobuf::{ColumnDef as PgColumnDef, ConstrType, Constraint, CreateStmt, RangeVar};
use pgorm_query::{
    Alias, ColumnDef, ForeignKey, ForeignKeyAction, ForeignKeyCreateStatement, Index,
    IndexCreateStatement, IntoTableName, Table, TableCreateStatement, TableName,
};
use std::collections::BTreeMap;

/// The statements that describe a table from outside its `CREATE TABLE`,
/// gathered before the table is built so they can be folded in.
#[derive(Default)]
pub(super) struct Attachments {
    pub(super) indexes: Vec<IndexCreateStatement>,
    pub(super) table_comment: Option<String>,
    pub(super) column_comments: BTreeMap<String, (usize, String)>,
}

/// The bare table name, which is the key every other statement — and the
/// entity transformer — refers to a table by.
pub(super) fn relname(stmt: &CreateStmt) -> &str {
    stmt.relation
        .as_ref()
        .map(|relation| relation.relname.as_str())
        .unwrap_or_default()
}

/// The table name, refusing a `CREATE TABLE` that somehow names nothing.
pub(super) fn name(stmt: &CreateStmt, at: usize) -> Result<String, Error> {
    match stmt.relation.as_ref() {
        Some(relation) if !relation.relname.is_empty() => Ok(relation.relname.clone()),
        _ => Err(unresolved("CREATE TABLE without a table name", at)),
    }
}

/// Bridge one `CREATE TABLE` into the statement the transformer reads.
// [spec:pgorm:sem:codegen.ddl.tables+2]
pub(super) fn build(
    stmt: &CreateStmt,
    at: usize,
    enums: &Enums,
    attachments: Attachments,
) -> Result<TableCreateStatement, Error> {
    let table_name = name(stmt, at)?;
    let target = reject_table_features(stmt, &table_name, at)?;

    let Attachments {
        indexes,
        table_comment,
        column_comments,
    } = attachments;

    let mut create = Table::create(target.clone());
    if stmt.if_not_exists {
        create.if_not_exists();
    }
    if let Some(comment) = table_comment {
        create.comment(comment);
    }

    let mut columns: Vec<Column> = Vec::new();
    let mut primary_key_columns: Vec<String> = Vec::new();
    let mut foreign_keys: Vec<ForeignKeyCreateStatement> = Vec::new();
    let mut table_indexes: Vec<IndexCreateStatement> = Vec::new();

    for element in &stmt.table_elts {
        match &element.node {
            Some(NodeEnum::ColumnDef(def)) => {
                let mut column = column(def, &target, &table_name, at, enums, &column_comments)?;
                if column.primary_key {
                    primary_key_columns.push(column.name.clone());
                }
                foreign_keys.extend(column.foreign_key.take());
                table_indexes.extend(column.unique_index.take());
                columns.push(column);
            }
            Some(NodeEnum::Constraint(constraint)) => {
                match table_constraint(constraint, &target, &table_name, at)? {
                    TableConstraint::Index(index) => {
                        if index.is_primary_key() {
                            primary_key_columns.extend(index.get_index_spec().get_column_names());
                        }
                        table_indexes.push(*index);
                    }
                    TableConstraint::ForeignKey(foreign_key) => foreign_keys.push(*foreign_key),
                }
            }
            Some(NodeEnum::TableLikeClause(_)) => {
                return Err(unsupported(
                    format!("a LIKE clause on table `{table_name}`"),
                    at,
                ));
            }
            _ => {
                return Err(unsupported(
                    format!("a table element of table `{table_name}`"),
                    at,
                ));
            }
        }
    }

    for (commented, (comment_at, _)) in &column_comments {
        if !columns.iter().any(|column| column.name == *commented) {
            return Err(unresolved(
                format!("table `{table_name}` has no column `{commented}`"),
                *comment_at,
            ));
        }
    }

    // A primary-key column is NOT NULL by Postgres' own rule, spelled out or
    // not; the entity model reads nullability off the column alone.
    for mut column in columns {
        if !column.not_null && primary_key_columns.contains(&column.name) {
            column.def.not_null();
        }
        create.col(column.def);
    }

    for mut index in table_indexes.into_iter().chain(indexes) {
        if index.is_primary_key() {
            create.primary_key(&mut index);
        } else {
            create.index(&mut index);
        }
    }
    for mut foreign_key in foreign_keys {
        create.foreign_key(&mut foreign_key);
    }
    Ok(create.take())
}

/// Refuse every `CREATE TABLE` feature the entity model has no place for, and
/// hand back the table name the rest of the build hangs off.
// [spec:pgorm:req:codegen.ddl.unsupported+1]
fn reject_table_features(
    stmt: &CreateStmt,
    table_name: &str,
    at: usize,
) -> Result<TableName, Error> {
    let on = |what: &str| unsupported(format!("{what} on table `{table_name}`"), at);
    let Some(relation) = stmt.relation.as_ref() else {
        return Err(unresolved("CREATE TABLE without a table name", at));
    };
    if relation.relpersistence != "p" {
        return Err(on("a temporary or unlogged table"));
    }
    if !stmt.inh_relations.is_empty() {
        return Err(on("an INHERITS clause"));
    }
    if stmt.partbound.is_some() {
        return Err(on("a PARTITION OF clause"));
    }
    if stmt.partspec.is_some() {
        return Err(on("a PARTITION BY clause"));
    }
    if stmt.of_typename.is_some() {
        return Err(on("an OF type clause"));
    }
    if !stmt.constraints.is_empty() {
        return Err(on("an inherited constraint"));
    }
    if !stmt.options.is_empty() {
        return Err(on("a WITH storage option"));
    }
    if !stmt.tablespacename.is_empty() {
        return Err(on("a TABLESPACE clause"));
    }
    if !stmt.access_method.is_empty() {
        return Err(on("a USING access method"));
    }
    if stmt.oncommit != pg_query::protobuf::OnCommitAction::OncommitNoop as i32 {
        return Err(on("an ON COMMIT clause"));
    }
    table_target(relation, &format!("table `{table_name}`"), at)
}

/// A `RangeVar` as the table name a DDL statement targets. Postgres has no
/// cross-database reference to render, so a catalog-qualified name is refused
/// rather than quietly reduced to its schema and table.
// [spec:pgorm:sem:codegen.ddl.tables+2]
fn table_target(relation: &RangeVar, context: &str, at: usize) -> Result<TableName, Error> {
    let table = Alias::new(relation.relname.as_str());
    match (relation.catalogname.as_str(), relation.schemaname.as_str()) {
        ("", "") => Ok(table.into_table_name()),
        ("", schema) => Ok((Alias::new(schema), table).into_table_name()),
        _ => Err(unsupported(
            format!("a cross-database table name on {context}"),
            at,
        )),
    }
}

/// A column definition, the facts the table needs to finish it, and the
/// foreign key a column-level `REFERENCES` declares alongside it.
struct Column {
    name: String,
    def: ColumnDef,
    not_null: bool,
    primary_key: bool,
    unique_index: Option<IndexCreateStatement>,
    foreign_key: Option<ForeignKeyCreateStatement>,
}

// [spec:pgorm:sem:codegen.ddl.tables+2]
fn column(
    def: &PgColumnDef,
    target: &TableName,
    table_name: &str,
    at: usize,
    enums: &Enums,
    comments: &BTreeMap<String, (usize, String)>,
) -> Result<Column, Error> {
    let column_name = def.colname.as_str();
    if column_name.is_empty() {
        return Err(unresolved(
            format!("table `{table_name}` has a column without a name"),
            at,
        ));
    }
    let context = format!("column `{table_name}`.`{column_name}`");
    let on = |what: &str| unsupported(format!("{what} on {context}"), at);
    if !def.storage.is_empty() || !def.storage_name.is_empty() {
        return Err(on("a STORAGE clause"));
    }
    if !def.compression.is_empty() {
        return Err(on("a COMPRESSION clause"));
    }
    if !def.identity.is_empty() {
        return Err(on("an identity clause"));
    }
    if !def.generated.is_empty() {
        return Err(on("a GENERATED clause"));
    }
    if def.coll_clause.is_some() {
        return Err(on("a COLLATE clause"));
    }
    if def.raw_default.is_some() || def.cooked_default.is_some() {
        return Err(on("a DEFAULT clause"));
    }
    if !def.fdwoptions.is_empty() {
        return Err(on("a column option"));
    }

    let Some(type_name) = def.type_name.as_ref() else {
        return Err(unresolved(format!("{context} has no type"), at));
    };
    let kind = types::column_kind(type_name, enums, &context, at)?;
    let mut column = ColumnDef::new_with_type(Alias::new(column_name), kind.col_type);
    if kind.auto_increment {
        column.auto_increment();
    }

    let mut not_null = def.is_not_null;
    let mut nullable = false;
    let mut primary_key = false;
    let mut unique_index = None;
    let mut foreign_key = None;
    for node in &def.constraints {
        let Some(NodeEnum::Constraint(constraint)) = &node.node else {
            return Err(on("a column constraint"));
        };
        reject_constraint_features(constraint, &context, at)?;
        match constraint_type(constraint, &context, at)? {
            ConstrType::ConstrNotnull => not_null = true,
            ConstrType::ConstrNull => nullable = true,
            ConstrType::ConstrPrimary => primary_key = true,
            // Postgres implements a UNIQUE column constraint as a one-column
            // unique index, and that index is where the entity model reads
            // uniqueness from — a `ColumnSpec::UniqueKey` would be discarded.
            ConstrType::ConstrUnique => {
                let mut index = Index::create(target.clone(), Alias::new(column_name));
                if !constraint.conname.is_empty() {
                    index.name(constraint.conname.as_str());
                }
                index.unique();
                if constraint.nulls_not_distinct {
                    index.nulls_not_distinct();
                }
                unique_index = Some(index.take());
            }
            ConstrType::ConstrForeign => {
                if foreign_key.is_some() {
                    return Err(on("a second REFERENCES clause"));
                }
                foreign_key = Some(references(
                    constraint,
                    target,
                    &[column_name.to_owned()],
                    &context,
                    at,
                )?);
            }
            other => return Err(on(constraint_kind(other))),
        }
    }
    if not_null {
        column.not_null();
    } else if nullable {
        column.null();
    }
    if primary_key {
        column.primary_key();
    }
    if let Some((_, comment)) = comments.get(column_name) {
        column.comment(comment.as_str());
    }
    Ok(Column {
        name: column_name.to_owned(),
        def: column.take(),
        not_null,
        primary_key,
        unique_index,
        foreign_key,
    })
}

/// What a table-level constraint becomes once bridged.
enum TableConstraint {
    Index(Box<IndexCreateStatement>),
    ForeignKey(Box<ForeignKeyCreateStatement>),
}

// [spec:pgorm:sem:codegen.ddl.tables+2]
fn table_constraint(
    constraint: &Constraint,
    target: &TableName,
    table_name: &str,
    at: usize,
) -> Result<TableConstraint, Error> {
    let context = format!("table `{table_name}`");
    let on = |what: &str| unsupported(format!("{what} on {context}"), at);
    reject_constraint_features(constraint, &context, at)?;
    match constraint_type(constraint, &context, at)? {
        kind @ (ConstrType::ConstrPrimary | ConstrType::ConstrUnique) => {
            let columns = types::idents(&constraint.keys)
                .ok_or_else(|| on("a constraint over computed keys"))?;
            let mut columns = columns.into_iter();
            let Some(first) = columns.next() else {
                return Err(on("a key constraint over no columns"));
            };
            let mut index = Index::create(target.clone(), Alias::new(first));
            if !constraint.conname.is_empty() {
                index.name(constraint.conname.as_str());
            }
            for column in columns {
                index.col(Alias::new(column));
            }
            if matches!(kind, ConstrType::ConstrPrimary) {
                index.primary();
            } else {
                index.unique();
            }
            if constraint.nulls_not_distinct {
                index.nulls_not_distinct();
            }
            Ok(TableConstraint::Index(Box::new(index.take())))
        }
        ConstrType::ConstrForeign => {
            let columns = types::idents(&constraint.fk_attrs)
                .ok_or_else(|| on("a foreign key over computed columns"))?;
            if columns.is_empty() {
                return Err(on("a foreign key over no columns"));
            }
            Ok(TableConstraint::ForeignKey(Box::new(references(
                constraint, target, &columns, &context, at,
            )?)))
        }
        other => Err(on(constraint_kind(other))),
    }
}

/// A foreign key over `columns` of `target`, with the referenced table, columns
/// and actions the constraint declares.
// [spec:pgorm:sem:codegen.ddl.tables+2]
fn references(
    constraint: &Constraint,
    target: &TableName,
    columns: &[String],
    context: &str,
    at: usize,
) -> Result<ForeignKeyCreateStatement, Error> {
    let Some(pktable) = constraint.pktable.as_ref() else {
        return Err(unsupported(
            format!("a foreign key naming no table on {context}"),
            at,
        ));
    };
    let ref_columns = types::idents(&constraint.pk_attrs)
        .ok_or_else(|| unsupported(format!("a foreign key over computed keys on {context}"), at))?;
    if ref_columns.is_empty() {
        return Err(unsupported(
            format!("REFERENCES without a column list on {context}"),
            at,
        ));
    }
    if columns.len() != ref_columns.len() {
        return Err(unresolved(
            format!(
                "a foreign key mapping {} columns onto {} on {context}",
                columns.len(),
                ref_columns.len()
            ),
            at,
        ));
    }
    if !matches!(constraint.fk_matchtype.as_str(), "" | "s") {
        return Err(unsupported(format!("a MATCH clause on {context}"), at));
    }
    if !constraint.fk_del_set_cols.is_empty() {
        return Err(unsupported(
            format!("ON DELETE SET NULL with a column list on {context}"),
            at,
        ));
    }
    let ref_table = table_target(pktable, context, at)?;
    let mut pairs = columns.iter().zip(ref_columns.iter());
    let Some((column, ref_column)) = pairs.next() else {
        return Err(unresolved(
            format!("a foreign key over no columns on {context}"),
            at,
        ));
    };
    let mut created = ForeignKey::create(
        target.clone(),
        Alias::new(column.as_str()),
        ref_table,
        Alias::new(ref_column.as_str()),
    );
    for (column, ref_column) in pairs {
        created.col(Alias::new(column.as_str()), Alias::new(ref_column.as_str()));
    }
    named(&mut created, constraint);
    if let Some(action) = action(&constraint.fk_upd_action, "UPDATE", context, at)? {
        created.on_update(action);
    }
    if let Some(action) = action(&constraint.fk_del_action, "DELETE", context, at)? {
        created.on_delete(action);
    }
    Ok(created.take())
}

/// A referential action code. `NO ACTION` is Postgres' default and carries no
/// entity meaning, so it reads as no action declared.
// [spec:pgorm:sem:codegen.ddl.tables+2]
fn action(
    code: &str,
    clause: &str,
    context: &str,
    at: usize,
) -> Result<Option<ForeignKeyAction>, Error> {
    match code {
        "" | "a" => Ok(None),
        "r" => Ok(Some(ForeignKeyAction::Restrict)),
        "c" => Ok(Some(ForeignKeyAction::Cascade)),
        "n" => Ok(Some(ForeignKeyAction::SetNull)),
        "d" => Ok(Some(ForeignKeyAction::SetDefault)),
        other => Err(unsupported(
            format!("ON {clause} action `{other}` on {context}"),
            at,
        )),
    }
}

fn named(created: &mut ForeignKeyCreateStatement, constraint: &Constraint) {
    if !constraint.conname.is_empty() {
        created.name(constraint.conname.as_str());
    }
}

/// Constraint attributes that survive into no part of the entity model.
// [spec:pgorm:req:codegen.ddl.unsupported+1]
fn reject_constraint_features(
    constraint: &Constraint,
    context: &str,
    at: usize,
) -> Result<(), Error> {
    let on = |what: &str| unsupported(format!("{what} on {context}"), at);
    if constraint.deferrable || constraint.initdeferred {
        return Err(on("a deferrable constraint"));
    }
    if constraint.is_no_inherit {
        return Err(on("a NO INHERIT constraint"));
    }
    if !constraint.including.is_empty() {
        return Err(on("an INCLUDE clause"));
    }
    if !constraint.options.is_empty() || !constraint.indexspace.is_empty() {
        return Err(on("a constraint storage option"));
    }
    if !constraint.indexname.is_empty() {
        return Err(on("a USING INDEX clause"));
    }
    if !constraint.access_method.is_empty() {
        return Err(on("a constraint access method"));
    }
    if constraint.where_clause.is_some() {
        return Err(on("a partial constraint"));
    }
    Ok(())
}

fn constraint_type(constraint: &Constraint, context: &str, at: usize) -> Result<ConstrType, Error> {
    ConstrType::try_from(constraint.contype)
        .map_err(|_| unsupported(format!("an unrecognised constraint on {context}"), at))
}

/// How a constraint the bridge does not carry was written.
// [spec:pgorm:req:codegen.ddl.unsupported+1]
fn constraint_kind(kind: ConstrType) -> &'static str {
    match kind {
        ConstrType::ConstrDefault => "a DEFAULT clause",
        ConstrType::ConstrCheck => "a CHECK constraint",
        ConstrType::ConstrGenerated => "a GENERATED clause",
        ConstrType::ConstrIdentity => "an identity clause",
        ConstrType::ConstrExclusion => "an EXCLUDE constraint",
        ConstrType::ConstrAttrDeferrable
        | ConstrType::ConstrAttrNotDeferrable
        | ConstrType::ConstrAttrDeferred
        | ConstrType::ConstrAttrImmediate => "a deferrable constraint",
        _ => "an unrecognised constraint",
    }
}
