use super::{Declared, types, unresolved, unsupported};
use crate::{Error, TableIdent, util::repeated_column};
use pg_query::NodeEnum;
use pg_query::protobuf::{
    CollateClause, ColumnDef as PgColumnDef, ConstrType, Constraint, CreateStmt, RangeVar,
};
use pgorm_query::{
    Collation, ColumnDef, Enforcement, ForeignKey, ForeignKeyAction, ForeignKeyCreateStatement,
    IdentityGeneration, IntoCollation, IntoTableName, Name, Primary, Table, TableCreateStatement,
    TableKey, TableName, Unique,
};
use std::collections::BTreeMap;

/// The statements that describe a table from outside its `CREATE TABLE`,
/// gathered before the table is built so they can be folded in.
#[derive(Default)]
pub(super) struct Attachments {
    pub(super) indexes: Vec<TableKey<Unique>>,
    pub(super) table_comment: Option<String>,
    pub(super) column_comments: BTreeMap<String, (usize, String)>,
}

/// The identity a `CREATE TABLE` declares — schema and all — which is the key
/// every other statement, and the entity transformer, refers to a table by.
// [spec:pgorm:sem:codegen.ddl.objects+8]
pub(super) fn ident(stmt: &CreateStmt) -> TableIdent {
    stmt.relation
        .as_ref()
        .map(|relation| TableIdent {
            table: relation.relname.clone(),
            schema: schema_of(relation),
        })
        .unwrap_or_default()
}

/// The schema a DDL statement's name is qualified with, if any.
// [spec:pgorm:sem:codegen.ddl.objects+8]
pub(super) fn schema_of(relation: &RangeVar) -> Option<String> {
    Some(relation.schemaname.clone()).filter(|schema| !schema.is_empty())
}

/// The table name, refusing a `CREATE TABLE` that somehow names nothing.
pub(super) fn name(stmt: &CreateStmt, at: usize) -> Result<String, Error> {
    match stmt.relation.as_ref() {
        Some(relation) if !relation.relname.is_empty() => Ok(relation.relname.clone()),
        _ => Err(unresolved("CREATE TABLE without a table name", at)),
    }
}

/// Bridge one `CREATE TABLE` into the statement the transformer reads.
// [spec:pgorm:sem:codegen.ddl.tables+8]
pub(super) fn build(
    stmt: &CreateStmt,
    at: usize,
    declared: &Declared,
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
    let mut primary_key: Option<TableKey<Primary>> = None;
    let mut foreign_keys: Vec<ForeignKeyCreateStatement> = Vec::new();
    let mut unique_keys: Vec<TableKey<Unique>> = Vec::new();
    let mut not_nulls: Vec<(String, NotNull)> = Vec::new();

    // A second primary key is refused rather than handed to the statement,
    // whose one slot would keep the later key and drop the earlier in silence.
    let mut declare = |key: TableKey<Primary>| match primary_key.replace(key) {
        Some(_) => Err(second_primary_key(&table_name, at)),
        None => Ok(()),
    };
    for element in &stmt.table_elts {
        match &element.node {
            Some(NodeEnum::ColumnDef(def)) => {
                let mut column = column(def, &target, &table_name, at, declared)?;
                if column.primary_key {
                    declare(TableKey::new(Name::runtime(column.name.as_str())))?;
                }
                foreign_keys.extend(column.foreign_key.take());
                unique_keys.extend(column.unique_key.take());
                columns.push(column);
            }
            Some(NodeEnum::Constraint(constraint)) => {
                match table_constraint(constraint, &target, &table_name, at)? {
                    TableConstraint::Primary(key) => declare(*key)?,
                    TableConstraint::Unique(key) => unique_keys.push(*key),
                    TableConstraint::ForeignKey(foreign_key) => foreign_keys.push(*foreign_key),
                    TableConstraint::NotNull(column, not_null) => {
                        not_nulls.push((column, not_null))
                    }
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

    // A table-level NOT NULL is the column's own constraint, which PostgreSQL
    // creates whichever spelling declared it, so it joins the column's.
    for (constrained, not_null) in not_nulls {
        let Some(column) = columns.iter_mut().find(|column| column.name == constrained) else {
            return Err(unresolved(
                format!("table `{table_name}` has no column `{constrained}`"),
                at,
            ));
        };
        column.declare_not_null(not_null, &table_name, at)?;
    }

    // A primary-key column is NOT NULL by Postgres' own rule, spelled out or
    // not; the entity model reads nullability off the column alone.
    let keyed = |name: &str| {
        primary_key.as_ref().is_some_and(|key| {
            key.get_columns()
                .iter()
                .any(|column| column.to_string() == name)
        })
    };
    for column in columns {
        let implied = keyed(&column.name);
        create.col(column.finish(implied, &column_comments));
    }

    if let Some(key) = primary_key {
        create.primary_key(key);
    }
    for key in unique_keys.into_iter().chain(indexes) {
        create.unique(key);
    }
    for foreign_key in foreign_keys {
        create.foreign_key(foreign_key);
    }
    Ok(create.take())
}

/// A table declaring a primary key beside the one it already has — on a second
/// column, as a table constraint beside a column's, or twice on one column.
/// PostgreSQL refuses every such table (42P16), and none is a composite key:
/// that is one `PRIMARY KEY (a, b)`.
// [spec:pgorm:req:codegen.ddl.unsupported+12]
fn second_primary_key(table_name: &str, at: usize) -> Error {
    unresolved(
        format!("table `{table_name}` declares more than one primary key"),
        at,
    )
}

/// Refuse every `CREATE TABLE` feature the entity model has no place for, and
/// hand back the table name the rest of the build hangs off.
// [spec:pgorm:req:codegen.ddl.unsupported+12]
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
// [spec:pgorm:sem:codegen.ddl.tables+8]
fn table_target(relation: &RangeVar, context: &str, at: usize) -> Result<TableName, Error> {
    let table = Name::runtime(relation.relname.as_str());
    match (relation.catalogname.as_str(), relation.schemaname.as_str()) {
        ("", "") => Ok(table.into_table_name()),
        ("", schema) => Ok((Name::runtime(schema), table).into_table_name()),
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
    not_null: Option<NotNull>,
    nullable: bool,
    identity: bool,
    primary_key: bool,
    unique_key: Option<TableKey<Unique>>,
    foreign_key: Option<ForeignKeyCreateStatement>,
}

/// A `NOT NULL` constraint as declared, at column or table level: the name it
/// was given, if any, and whether it is kept from inheriting tables.
#[derive(Default)]
struct NotNull {
    name: Option<String>,
    no_inherit: bool,
}

impl NotNull {
    fn of(constraint: &Constraint) -> Self {
        Self {
            name: Some(constraint.conname.clone()).filter(|name| !name.is_empty()),
            no_inherit: constraint.is_no_inherit,
        }
    }
}

impl Column {
    /// Join a declared `NOT NULL` to the column's. PostgreSQL makes one
    /// constraint of every `NOT NULL` a column has, keeping the one name any
    /// of them gives, and refuses two that name it differently or disagree on
    /// `NO INHERIT` (42601); so does this.
    // [spec:pgorm:sem:codegen.ddl.tables+8]
    fn declare_not_null(
        &mut self,
        declared: NotNull,
        table_name: &str,
        at: usize,
    ) -> Result<(), Error> {
        let context = format!("column `{table_name}`.`{}`", self.name);
        let Some(existing) = &mut self.not_null else {
            self.not_null = Some(declared);
            return Ok(());
        };
        if existing.no_inherit != declared.no_inherit {
            return Err(unresolved(
                format!("{context} declares NOT NULL both with and without NO INHERIT"),
                at,
            ));
        }
        match (&existing.name, declared.name) {
            (Some(kept), Some(other)) if *kept != other => Err(unresolved(
                format!("{context} names its NOT NULL constraint both `{kept}` and `{other}`"),
                at,
            )),
            (Some(_), _) | (None, None) => Ok(()),
            (None, named) => {
                existing.name = named;
                Ok(())
            }
        }
    }

    /// The finished column definition: its `NOT NULL` — declared, or implied
    /// by an identity or by `implied` — or else its `NULL`, then its comment.
    // [spec:pgorm:sem:codegen.ddl.tables+8]
    fn finish(mut self, implied: bool, comments: &BTreeMap<String, (usize, String)>) -> ColumnDef {
        match self.not_null {
            Some(NotNull { name, no_inherit }) => {
                self.def.not_null();
                if let Some(name) = name {
                    self.def.not_null_named(Name::runtime(name));
                }
                if no_inherit {
                    self.def.not_null_no_inherit();
                }
            }
            None if self.identity || implied => {
                self.def.not_null();
            }
            None if self.nullable => {
                self.def.null();
            }
            None => {}
        }
        if let Some((_, comment)) = comments.get(&self.name) {
            self.def.comment(comment.as_str());
        }
        self.def
    }
}

// [spec:pgorm:sem:codegen.ddl.tables+8]
fn column(
    def: &PgColumnDef,
    target: &TableName,
    table_name: &str,
    at: usize,
    declared: &Declared,
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
    if def.raw_default.is_some() || def.cooked_default.is_some() {
        return Err(on("a DEFAULT clause"));
    }
    if !def.fdwoptions.is_empty() {
        return Err(on("a column option"));
    }

    let Some(type_name) = def.type_name.as_ref() else {
        return Err(unresolved(format!("{context} has no type"), at));
    };
    let kind = types::column_kind(type_name, declared, &context, at)?;
    let mut column = ColumnDef::new_with_type(Name::runtime(column_name), kind.col_type);
    if kind.auto_increment {
        column.auto_increment();
    }
    if let Some(clause) = &def.coll_clause {
        column.collate(collation(clause, &context, at)?);
    }

    let mut built = Column {
        name: column_name.to_owned(),
        def: column,
        not_null: def.is_not_null.then(NotNull::default),
        nullable: false,
        identity: false,
        primary_key: false,
        unique_key: None,
        foreign_key: None,
    };
    let mut follows_references = false;
    for node in &def.constraints {
        let Some(NodeEnum::Constraint(constraint)) = &node.node else {
            return Err(on("a column constraint"));
        };
        let kind = constraint_type(constraint, &context, at)?;
        let after_references =
            std::mem::replace(&mut follows_references, kind == ConstrType::ConstrForeign);
        if kind == ConstrType::ConstrIdentity {
            // An identity column is NOT NULL whether or not it says so.
            match identity(constraint, &context, at)? {
                IdentityGeneration::Always => built.def.identity(),
                IdentityGeneration::ByDefault => built.def.identity_by_default(),
            };
            built.identity = true;
            continue;
        }
        reject_constraint_features(constraint, &context, at)?;
        match kind {
            ConstrType::ConstrNotnull => {
                built.declare_not_null(NotNull::of(constraint), table_name, at)?;
            }
            ConstrType::ConstrNull => built.nullable = true,
            ConstrType::ConstrPrimary if built.primary_key => {
                return Err(second_primary_key(table_name, at));
            }
            ConstrType::ConstrPrimary => built.primary_key = true,
            // A column's UNIQUE is a one-column unique key: a key is the
            // table's, and Postgres implements this one as that key.
            ConstrType::ConstrUnique => {
                built.unique_key = Some(unique(
                    constraint,
                    key(constraint, Name::runtime(column_name)),
                ));
            }
            ConstrType::ConstrForeign => {
                if built.foreign_key.is_some() {
                    return Err(on("a second REFERENCES clause"));
                }
                built.foreign_key = Some(references(
                    constraint,
                    target,
                    &[column_name.to_owned()],
                    &context,
                    at,
                )?);
            }
            // An explicit ENFORCED on a REFERENCES states the default the
            // foreign key holds anyway, so the statement carries it as said.
            ConstrType::ConstrAttrEnforced if after_references => {
                if let Some(foreign_key) = built.foreign_key.as_mut() {
                    foreign_key.enforcement(Enforcement::Enforced);
                }
            }
            other => return Err(on(constraint_kind(constraint, other))),
        }
    }
    Ok(built)
}

/// The form of a column's `GENERATED { ALWAYS | BY DEFAULT } AS IDENTITY`.
/// Sequence options are refused: an entity declares which form generates the
/// column, not how its sequence counts.
// [spec:pgorm:sem:codegen.ddl.tables+8]
fn identity(
    constraint: &Constraint,
    context: &str,
    at: usize,
) -> Result<IdentityGeneration, Error> {
    if !constraint.options.is_empty() {
        return Err(unsupported(
            format!("sequence options on the identity of {context}"),
            at,
        ));
    }
    match constraint.generated_when.as_str() {
        "a" => Ok(IdentityGeneration::Always),
        "d" => Ok(IdentityGeneration::ByDefault),
        _ => Err(unsupported(
            format!("an unrecognised identity form on {context}"),
            at,
        )),
    }
}

/// A column's `COLLATE` clause as the collation it names: bare, or qualified
/// by one schema. A catalog-qualified name is a cross-database reference
/// Postgres does not implement, so it is refused as a table's would be.
// [spec:pgorm:sem:codegen.ddl.tables+8]
fn collation(clause: &CollateClause, context: &str, at: usize) -> Result<Collation, Error> {
    match types::idents(&clause.collname).as_deref() {
        Some([name]) => Ok(Name::runtime(name.as_str()).into_collation()),
        Some([schema, name]) => {
            Ok((Name::runtime(schema.as_str()), Name::runtime(name.as_str())).into_collation())
        }
        _ => Err(unsupported(
            format!("a cross-database collation name on {context}"),
            at,
        )),
    }
}

/// What a table-level constraint becomes once bridged.
enum TableConstraint {
    Primary(Box<TableKey<Primary>>),
    Unique(Box<TableKey<Unique>>),
    ForeignKey(Box<ForeignKeyCreateStatement>),
    NotNull(String, NotNull),
}

// [spec:pgorm:sem:codegen.ddl.tables+8]
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
            // PostgreSQL refuses the key (42701), and read as it stands it
            // is not the key written: see `repeated_column`.
            if let Some(column) = repeated_column(columns.iter().cloned()) {
                return Err(unresolved(
                    format!("{context} names column `{column}` twice in one key"),
                    at,
                ));
            }
            let mut columns = columns.into_iter();
            let Some(first) = columns.next() else {
                return Err(on("a key constraint over no columns"));
            };
            let columns = columns.map(|column| Name::runtime(column.as_str()));
            Ok(if kind == ConstrType::ConstrPrimary {
                TableConstraint::Primary(Box::new(
                    key(constraint, Name::runtime(first)).cols(columns),
                ))
            } else {
                let key = key(constraint, Name::runtime(first)).cols(columns);
                TableConstraint::Unique(Box::new(unique(constraint, key)))
            })
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
        // `[CONSTRAINT n] NOT NULL col [NO INHERIT] [NOT VALID]`: the
        // column's constraint, declared at table level. A `CREATE TABLE`
        // creates it valid whatever it says, its table having no rows to
        // check, so `NOT VALID` here describes nothing the catalog keeps.
        ConstrType::ConstrNotnull => match types::idents(&constraint.keys).as_deref() {
            Some([column]) => Ok(TableConstraint::NotNull(
                column.clone(),
                NotNull::of(constraint),
            )),
            _ => Err(on("a NOT NULL constraint over other than one column")),
        },
        other => Err(on(constraint_kind(constraint, other))),
    }
}

/// A `PRIMARY KEY` or `UNIQUE` constraint begun at its first column, under the
/// name it was declared with.
// [spec:pgorm:sem:codegen.ddl.tables+8]
fn key<K>(constraint: &Constraint, first: Name) -> TableKey<K> {
    let key = TableKey::new(first);
    if constraint.conname.is_empty() {
        key
    } else {
        key.name(Name::runtime(constraint.conname.as_str()))
    }
}

/// A unique key with the `NULLS NOT DISTINCT` its constraint was declared
/// with, which only a unique key can carry.
// [spec:pgorm:sem:codegen.ddl.tables+8]
fn unique(constraint: &Constraint, key: TableKey<Unique>) -> TableKey<Unique> {
    if constraint.nulls_not_distinct {
        key.nulls_not_distinct()
    } else {
        key
    }
}

/// A foreign key over `columns` of `target`, with the referenced table, columns
/// and actions the constraint declares.
// [spec:pgorm:sem:codegen.ddl.tables+8]
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
        Name::runtime(column.as_str()),
        ref_table,
        Name::runtime(ref_column.as_str()),
    );
    for (column, ref_column) in pairs {
        created.col(
            Name::runtime(column.as_str()),
            Name::runtime(ref_column.as_str()),
        );
    }
    named(&mut created, constraint);
    if let Some(action) = action(&constraint.fk_upd_action, "UPDATE", context, at)? {
        created.on_update(action);
    }
    if let Some(action) = action(&constraint.fk_del_action, "DELETE", context, at)? {
        created.on_delete(action);
    }
    Ok(created)
}

/// A referential action code. `NO ACTION` is Postgres' default and carries no
/// entity meaning, so it reads as no action declared.
// [spec:pgorm:sem:codegen.ddl.tables+8]
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
        created.name(Name::runtime(constraint.conname.as_str()));
    }
}

/// Constraint attributes that survive into no part of the entity model.
// [spec:pgorm:req:codegen.ddl.unsupported+12]
fn reject_constraint_features(
    constraint: &Constraint,
    context: &str,
    at: usize,
) -> Result<(), Error> {
    let on = |what: &str| unsupported(format!("{what} on {context}"), at);
    if constraint.deferrable || constraint.initdeferred {
        return Err(on("a deferrable constraint"));
    }
    // A NOT NULL kept from inheriting tables is the column's constraint all
    // the same, and its NO INHERIT rides on the statement.
    if constraint.is_no_inherit && constraint.contype != ConstrType::ConstrNotnull as i32 {
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
    // PostgreSQL 18's temporal keys: a key whose last column is a range that
    // may not overlap, and a foreign key matching on a period. The bridged
    // statement could carry either, and the entity cannot: its key and its
    // relations match by equality alone, so read as the other columns either
    // would claim a uniqueness or a join the schema does not have, and schema
    // generation from the entity would create the plain key.
    if constraint.without_overlaps {
        return Err(on("a WITHOUT OVERLAPS key"));
    }
    if constraint.fk_with_period || constraint.pk_with_period {
        return Err(on("a PERIOD foreign key"));
    }
    // A NOT ENFORCED foreign key admits rows that break it, and the relation
    // an entity would read it as cannot say so: schema generation would
    // create the key enforced, and a load would meet orphans the relation
    // promises are not there. Only a foreign key carries enforcement this
    // early: the grammar sets it on every one and leaves it unset on the
    // keys that cannot be NOT ENFORCED.
    if constraint.contype == ConstrType::ConstrForeign as i32 && !constraint.is_enforced {
        return Err(on("a NOT ENFORCED constraint"));
    }
    Ok(())
}

fn constraint_type(constraint: &Constraint, context: &str, at: usize) -> Result<ConstrType, Error> {
    ConstrType::try_from(constraint.contype)
        .map_err(|_| unsupported(format!("an unrecognised constraint on {context}"), at))
}

/// How a constraint the bridge does not carry was written.
// [spec:pgorm:req:codegen.ddl.unsupported+12]
fn constraint_kind(constraint: &Constraint, kind: ConstrType) -> &'static str {
    match kind {
        ConstrType::ConstrDefault => "a DEFAULT clause",
        ConstrType::ConstrCheck => "a CHECK constraint",
        // PostgreSQL 18 reads a bare `GENERATED ALWAYS AS (...)` as VIRTUAL
        // too, so the grammar's kind is the answer, not the keyword's
        // presence.
        ConstrType::ConstrGenerated if constraint.generated_kind == "v" => {
            "a VIRTUAL generated column"
        }
        ConstrType::ConstrGenerated => "a GENERATED clause",
        ConstrType::ConstrIdentity => "an identity clause",
        ConstrType::ConstrExclusion => "an EXCLUDE constraint",
        ConstrType::ConstrAttrDeferrable
        | ConstrType::ConstrAttrNotDeferrable
        | ConstrType::ConstrAttrDeferred
        | ConstrType::ConstrAttrImmediate => "a deferrable constraint",
        ConstrType::ConstrAttrNotEnforced => "a NOT ENFORCED constraint",
        ConstrType::ConstrAttrEnforced => "an ENFORCED clause",
        _ => "an unrecognised constraint",
    }
}
