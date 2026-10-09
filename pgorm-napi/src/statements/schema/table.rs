//! `CREATE TABLE` with its columns, keys, foreign keys and `CHECK`s; the
//! statements that drop, rename and empty a table; and comments.

use neon::prelude::*;
use pgorm::pgorm_query::{
    Comment, ConstraintRenameStatement, DropBehavior, ForeignKeyCreateStatement, Primary, Table,
    TableCreateStatement, TableKey, TableName, Unique,
};

use super::{
    super::{
        Node,
        args::{arg, name_at, refuse},
    },
    Ddl, Part,
    column::{check, column},
    options::{
        ACTIONS, BEHAVIOR, DEFERRABILITY, ENFORCEMENT, choice, columns, flag, get, name_option,
        names, object, relation, relation_at, relations_at, text,
    },
};

pub(super) const EXPORTS: &[(&str, super::super::Build)] = &[
    ("ddlCreateTable", create_table),
    ("ddlCreateTableColumn", create_table_column),
    ("ddlCreateTableIfNotExists", create_table_if_not_exists),
    ("ddlCreateTablePrimaryKey", create_table_primary_key),
    ("ddlCreateTableUnique", create_table_unique),
    ("ddlCreateTableForeignKey", create_table_foreign_key),
    ("ddlCreateTableCheck", create_table_check),
    ("ddlDropTable", drop_table),
    ("ddlRenameTable", rename_table),
    ("ddlRenameColumn", rename_column),
    ("ddlRenameConstraint", rename_constraint),
    ("ddlTruncateTable", truncate_table),
    ("ddlCommentOnTable", comment_on_table),
    ("ddlCommentOnColumn", comment_on_column),
];

/// The options a key takes, both kinds.
const KEY_OPTIONS: &[&str] = &["name", "include", "deferrability", "withoutOverlaps"];

/// A key of either kind over the columns at `index`, with the options object
/// after them: a name, `INCLUDE`d columns, its deferrability and PostgreSQL
/// 18's `WITHOUT OVERLAPS` column, which pgorm-query writes last.
// [spec:pgorm:req:napi.schema-tables]
fn key<'cx, K>(
    cx: &mut FunctionContext<'cx>,
    index: usize,
    extra: &[&str],
) -> NeonResult<(TableKey<K>, Option<Handle<'cx, JsObject>>)> {
    let value = arg(cx, index);
    let (first, rest) = columns(cx, value, "a key")?;
    let mut known = KEY_OPTIONS.to_vec();
    known.extend_from_slice(extra);
    let options = object(cx, index + 1, &known)?;
    let mut key = TableKey::new(first).cols(rest);
    if let Some(name) = name_option(cx, options, "name")? {
        key = key.name(name);
    }
    if let Some(include) = get(cx, options, "include")? {
        key = key.include(names(cx, include, "a key's include list")?);
    }
    if let Some(deferrability) = choice(cx, options, "deferrability", DEFERRABILITY)? {
        key = key.deferrability(deferrability);
    }
    if let Some(period) = name_option(cx, options, "withoutOverlaps")? {
        key = key.without_overlaps(period);
    }
    Ok((key, options))
}

pub(super) fn primary_key(cx: &mut FunctionContext, index: usize) -> NeonResult<TableKey<Primary>> {
    key(cx, index, &[]).map(|(key, _)| key)
}

/// A unique key, which alone takes `NULLS NOT DISTINCT`.
// [spec:pgorm:req:napi.schema-tables]
pub(super) fn unique_key(cx: &mut FunctionContext, index: usize) -> NeonResult<TableKey<Unique>> {
    let (key, options) = key::<Unique>(cx, index, &["nullsNotDistinct"])?;
    Ok(if flag(cx, options, "nullsNotDistinct")? {
        key.nulls_not_distinct()
    } else {
        key
    })
}

/// A foreign key of `table`'s: its columns at `index`, the table they
/// reference and its columns after them, paired one to one, and the options
/// object — a name, the referential actions, deferrability, enforcement and
/// PostgreSQL 18's `PERIOD` pair, which pgorm-query writes last on both
/// sides.
// [spec:pgorm:req:napi.schema-tables]
pub(super) fn foreign_key<'cx>(
    cx: &mut FunctionContext<'cx>,
    table: TableName,
    index: usize,
    extra: &[&str],
) -> NeonResult<(ForeignKeyCreateStatement, Option<Handle<'cx, JsObject>>)> {
    let value = arg(cx, index);
    let (first, rest) = columns(cx, value, "a foreign key")?;
    let references = relation_at(cx, index + 1)?;
    let value = arg(cx, index + 2);
    let (ref_first, ref_rest) = columns(cx, value, "a foreign key's referenced list")?;
    if rest.len() != ref_rest.len() {
        return refuse(
            cx,
            "a foreign key names as many referenced columns as columns, paired in order",
        );
    }
    let mut known = vec![
        "name",
        "onDelete",
        "onUpdate",
        "deferrability",
        "enforcement",
        "period",
    ];
    known.extend_from_slice(extra);
    let options = object(cx, index + 3, &known)?;
    let mut key = ForeignKeyCreateStatement::new(table, first, references, ref_first);
    for (column, referenced) in rest.into_iter().zip(ref_rest) {
        key.col(column, referenced);
    }
    if let Some(name) = name_option(cx, options, "name")? {
        key.name(name);
    }
    if let Some(action) = choice(cx, options, "onDelete", ACTIONS)? {
        key.on_delete(action);
    }
    if let Some(action) = choice(cx, options, "onUpdate", ACTIONS)? {
        key.on_update(action);
    }
    if let Some(deferrability) = choice(cx, options, "deferrability", DEFERRABILITY)? {
        key.deferrability(deferrability);
    }
    if let Some(enforcement) = choice(cx, options, "enforcement", ENFORCEMENT)? {
        key.enforcement(enforcement);
    }
    if let Some(period) = get(cx, options, "period")? {
        let pair = names(cx, period, "a PERIOD pair")?;
        let [column, referenced] = <[_; 2]>::try_from(pair).or_else(|_| {
            refuse(
                cx,
                "a PERIOD pair is [column, referenced column], one name each side",
            )
        })?;
        key.period(column, referenced);
    }
    Ok((key, options))
}

fn create(cx: &mut FunctionContext) -> NeonResult<TableCreateStatement> {
    super::receiver(cx, "a CREATE TABLE", |part| match part {
        Part::Statement(Ddl::CreateTable(create)) => Ok(create),
        other => Err(other.describe()),
    })
}

/// `ddlCreateTable(table)`.
// [spec:pgorm:req:napi.schema-tables]
fn create_table(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = relation_at(cx, 0)?;
    Ok(Ddl::CreateTable(Table::create(table)).into())
}

/// `ddlCreateTableColumn(create, column)`: a column, which needs its type.
// [spec:pgorm:req:napi.schema-tables]
fn create_table_column(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut create = create(cx)?;
    let value = arg(cx, 1);
    let column = column(cx, value)?;
    if column.get_column_type().is_none() {
        return refuse(cx, "a column a table is created with needs a type");
    }
    create.col(column);
    Ok(Ddl::CreateTable(create).into())
}

fn create_table_if_not_exists(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut create = create(cx)?;
    create.if_not_exists();
    Ok(Ddl::CreateTable(create).into())
}

/// `ddlCreateTablePrimaryKey(create, columns, options)`: the table's one
/// primary key, a later call replacing it.
fn create_table_primary_key(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut create = create(cx)?;
    let key = primary_key(cx, 1)?;
    create.primary_key(key);
    Ok(Ddl::CreateTable(create).into())
}

fn create_table_unique(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut create = create(cx)?;
    let key = unique_key(cx, 1)?;
    create.unique(key);
    Ok(Ddl::CreateTable(create).into())
}

fn create_table_foreign_key(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut create = create(cx)?;
    let table = create.get_table_name().clone();
    let (key, _) = foreign_key(cx, table, 1, &[])?;
    create.foreign_key(key);
    Ok(Ddl::CreateTable(create).into())
}

fn create_table_check(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut create = create(cx)?;
    let (check, _) = check(cx, 1, &[])?;
    create.check(check);
    Ok(Ddl::CreateTable(create).into())
}

/// `ddlDropTable(tables, { ifExists, behavior })`.
// [spec:pgorm:req:napi.schema-tables]
fn drop_table(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (first, rest) = relations_at(cx, 0)?;
    let options = object(cx, 1, &["ifExists", "behavior"])?;
    let mut drop = Table::drop(first);
    for table in rest {
        drop.table(table);
    }
    if flag(cx, options, "ifExists")? {
        drop.if_exists();
    }
    match choice(cx, options, "behavior", BEHAVIOR)? {
        Some(DropBehavior::Cascade) => {
            drop.cascade();
        }
        Some(DropBehavior::Restrict) => {
            drop.restrict();
        }
        None => {}
    }
    Ok(Ddl::DropTable(drop).into())
}

/// `ddlRenameTable(table, name)`: the new name is bare, `RENAME TO` leaving
/// the table in its schema.
fn rename_table(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = relation_at(cx, 0)?;
    let name = name_at(cx, 1)?;
    Ok(Ddl::RenameTable(Table::rename(table, name)).into())
}

fn rename_column(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = relation_at(cx, 0)?;
    let from = name_at(cx, 1)?;
    let to = name_at(cx, 2)?;
    Ok(Ddl::RenameColumn(Table::rename_column(table, from, to)).into())
}

fn rename_constraint(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = relation_at(cx, 0)?;
    let from = name_at(cx, 1)?;
    let to = name_at(cx, 2)?;
    Ok(Ddl::RenameConstraint(ConstraintRenameStatement::new(table, from, to)).into())
}

fn truncate_table(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = relation_at(cx, 0)?;
    Ok(Ddl::Truncate(Table::truncate(table)).into())
}

/// `ddlCommentOnTable(table, text)`: the text a literal pgorm-query escapes.
// [spec:pgorm:req:napi.schema-sequences]
fn comment_on_table(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = relation_at(cx, 0)?;
    let comment = arg(cx, 1);
    let comment = text(cx, comment, "a comment")?;
    Ok(Ddl::Comment(Comment::on_table(table, comment)).into())
}

fn comment_on_column(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = arg(cx, 0);
    let table = relation(cx, table)?;
    let column = name_at(cx, 1)?;
    let comment = arg(cx, 2);
    let comment = text(cx, comment, "a comment")?;
    Ok(Ddl::Comment(Comment::on_column(table, column, comment)).into())
}
