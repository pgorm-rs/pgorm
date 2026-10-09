//! `ALTER TABLE` over pgorm-query's typestate: `alterTable(table)` names the
//! table and nothing more, as `PendingTableAlter` does, and its first action
//! gives the statement, which takes more. Each action is the pgorm-query
//! method of the same name, on the pending alter or the statement alike.

use neon::prelude::*;
use pgorm::pgorm_query::{
    ColumnDef, ColumnSpec, ConstraintChange, ConstraintDrop, DropBehavior, NotNullConstraint,
    PendingTableAlter, Table, TableAlterStatement, TableName,
};

use super::{
    super::{
        Node,
        args::{arg, expression, name_at, refuse},
    },
    Ddl, Part,
    column::{check, column},
    options::{BEHAVIOR, choice, flag, name_option, object, pick, relation_at},
    table::{foreign_key, primary_key, unique_key},
};

pub(super) const EXPORTS: &[(&str, super::super::Build)] = &[
    ("ddlAlterTable", alter_table),
    ("ddlAlterAddColumn", add_column),
    ("ddlAlterModifyColumn", modify_column),
    ("ddlAlterDropColumn", drop_column),
    ("ddlAlterAddPrimaryKey", add_primary_key),
    ("ddlAlterAddUnique", add_unique),
    ("ddlAlterAddForeignKey", add_foreign_key),
    ("ddlAlterAddCheck", add_check),
    ("ddlAlterAddNotNull", add_not_null),
    ("ddlAlterDropConstraint", drop_constraint),
    ("ddlAlterValidateConstraint", validate_constraint),
    ("ddlAlterAlterConstraint", alter_constraint),
    ("ddlAlterSetExpression", set_expression),
    ("ddlAlterDropExpression", drop_expression),
];

/// An `ALTER TABLE` before or after its first action.
enum Alter {
    Pending(PendingTableAlter),
    Statement(TableAlterStatement),
}

fn receiver(cx: &mut FunctionContext) -> NeonResult<(TableName, Alter)> {
    super::receiver(cx, "an ALTER TABLE", |part| match part {
        Part::PendingAlterTable(table, pending) => Ok((table, Alter::Pending(pending))),
        Part::Statement(Ddl::AlterTable(table, statement)) => {
            Ok((table, Alter::Statement(statement)))
        }
        other => Err(other.describe()),
    })
}

/// Apply the pgorm-query action `$method` to an alter, pending or not.
macro_rules! act {
    ($table:expr, $alter:expr, $method:ident($($arg:expr),*)) => {{
        let statement = match $alter {
            Alter::Pending(pending) => pending.$method($($arg),*),
            Alter::Statement(mut statement) => {
                statement.$method($($arg),*);
                statement
            }
        };
        Ok(Ddl::AlterTable($table, statement).into())
    }};
}

/// `ddlAlterTable(table)`: the table, with no action yet.
// [spec:pgorm:req:napi.schema-alter]
fn alter_table(cx: &mut FunctionContext) -> NeonResult<Node> {
    let table = relation_at(cx, 0)?;
    Ok(Part::PendingAlterTable(table.clone(), Table::alter(table)).into())
}

/// The column at `index`, which an added column needs a type for.
fn typed_column(cx: &mut FunctionContext, index: usize) -> NeonResult<ColumnDef> {
    let value = arg(cx, index);
    let column = column(cx, value)?;
    if column.get_column_type().is_none() {
        return refuse(cx, "an added column needs a type");
    }
    Ok(column)
}

fn add_column(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let column = typed_column(cx, 1)?;
    let options = object(cx, 2, &["ifNotExists"])?;
    if flag(cx, options, "ifNotExists")? {
        act!(table, alter, add_column_if_not_exists(column))
    } else {
        act!(table, alter, add_column(column))
    }
}

/// `ddlAlterModifyColumn(alter, column)`: pgorm-query writes each aspect the
/// column carries as its own action. One it would leave out — a generated
/// expression, the serial family, a collation with no type to retype — is
/// refused rather than dropped.
// [spec:pgorm:req:napi.schema-alter]
fn modify_column(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let value = arg(cx, 1);
    let column = column(cx, value)?;
    for spec in column.get_column_spec() {
        match spec {
            ColumnSpec::Generated { .. } => {
                return refuse(
                    cx,
                    "a modified column cannot change its generated expression: use \
                     setExpression(..) or dropExpression(..)",
                );
            }
            ColumnSpec::AutoIncrement => {
                return refuse(
                    cx,
                    "a modified column cannot take the serial family: use identity(..)",
                );
            }
            _ => {}
        }
    }
    if column.get_collation().is_some() && column.get_column_type().is_none() {
        return refuse(
            cx,
            "a modified column changes its collation only with its type, which it does not give",
        );
    }
    if column.get_column_type().is_none() && column.get_column_spec().is_empty() {
        return refuse(
            cx,
            "a modified column changes something: its type, nullability, default, a CHECK or an \
             identity",
        );
    }
    act!(table, alter, modify_column(column))
}

fn drop_column(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let name = name_at(cx, 1)?;
    act!(table, alter, drop_column(name))
}

fn add_primary_key(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let key = primary_key(cx, 1)?;
    act!(table, alter, add_primary_key(key))
}

fn add_unique(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let key = unique_key(cx, 1)?;
    act!(table, alter, add_unique(key))
}

/// `ddlAlterAddForeignKey(alter, columns, references, refColumns, options)`:
/// a foreign key, `NOT VALID` when the options say, which leaves the rows
/// already there unchecked.
// [spec:pgorm:req:napi.schema-alter]
fn add_foreign_key(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let (key, options) = foreign_key(cx, table.clone(), 1, &["notValid"])?;
    let key = key.get_foreign_key().clone();
    if flag(cx, options, "notValid")? {
        act!(table, alter, add_foreign_key(key.not_valid()))
    } else {
        act!(table, alter, add_foreign_key(key))
    }
}

fn add_check(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let (check, options) = check(cx, 1, &["notValid"])?;
    if flag(cx, options, "notValid")? {
        act!(table, alter, add_check(check.not_valid()))
    } else {
        act!(table, alter, add_check(check))
    }
}

/// `ddlAlterAddNotNull(alter, column, { name, noInherit, notValid })`:
/// PostgreSQL 18's table-level `NOT NULL`, the one spelling with a place for
/// `NOT VALID`.
// [spec:pgorm:req:napi.schema-alter]
fn add_not_null(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let column = name_at(cx, 1)?;
    let options = object(cx, 2, &["name", "noInherit", "notValid"])?;
    let mut constraint = NotNullConstraint::new(column);
    if let Some(name) = name_option(cx, options, "name")? {
        constraint = constraint.name(name);
    }
    if flag(cx, options, "noInherit")? {
        constraint = constraint.no_inherit();
    }
    if flag(cx, options, "notValid")? {
        constraint = constraint.not_valid();
    }
    act!(table, alter, add_not_null(constraint))
}

/// `ddlAlterDropConstraint(alter, name, { ifExists, behavior })`: a
/// constraint of any kind, by name.
fn drop_constraint(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let name = name_at(cx, 1)?;
    let options = object(cx, 2, &["ifExists", "behavior"])?;
    let mut drop = ConstraintDrop::new(name);
    if flag(cx, options, "ifExists")? {
        drop = drop.if_exists();
    }
    drop = match choice(cx, options, "behavior", BEHAVIOR)? {
        Some(DropBehavior::Cascade) => drop.cascade(),
        Some(DropBehavior::Restrict) => drop.restrict(),
        None => drop,
    };
    act!(table, alter, drop_constraint(drop))
}

fn validate_constraint(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let name = name_at(cx, 1)?;
    act!(table, alter, validate_constraint(name))
}

/// `ddlAlterAlterConstraint(alter, name, change)`: whether a `NOT NULL`
/// passes to inheriting tables, or whether a foreign key is enforced.
// [spec:pgorm:req:napi.schema-alter]
fn alter_constraint(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let name = name_at(cx, 1)?;
    let change = arg(cx, 2);
    let change = pick(
        cx,
        change,
        "a constraint's change",
        &[
            ("inherit", ConstraintChange::Inherit),
            ("noInherit", ConstraintChange::NoInherit),
            ("enforced", ConstraintChange::Enforced),
            ("notEnforced", ConstraintChange::NotEnforced),
        ],
    )?;
    act!(table, alter, alter_constraint(name, change))
}

/// `ddlAlterSetExpression(alter, column, expression)`: a generated column's
/// new expression, which the rows already written take.
// [spec:pgorm:req:napi.schema-alter]
fn set_expression(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let column = name_at(cx, 1)?;
    let expr = arg(cx, 2);
    let expr = expression(cx, expr)?;
    act!(table, alter, set_expression(column, expr))
}

fn drop_expression(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (table, alter) = receiver(cx)?;
    let column = name_at(cx, 1)?;
    let options = object(cx, 2, &["ifExists"])?;
    if flag(cx, options, "ifExists")? {
        act!(table, alter, drop_expression_if_exists(column))
    } else {
        act!(table, alter, drop_expression(column))
    }
}
