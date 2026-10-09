//! Sequences — created, altered over pgorm-query's typestate, dropped and
//! renamed — and extensions. A sequence's options are the vocabulary an
//! identity column's take.

use neon::prelude::*;
use pgorm::pgorm_query::{
    DropBehavior, PendingSequenceAlter, Sequence, SequenceAlterStatement, SequenceCreateStatement,
    SequenceOptions, SequenceType, extension::Extension,
};

use super::{
    super::{
        Node,
        args::{absent, arg, name_at, refuse},
    },
    Ddl, Part,
    options::{
        BEHAVIOR, choice, flag, get, integer, name_option, object, pick, relation_at, relations_at,
        sequence_options, text,
    },
};

pub(super) const EXPORTS: &[(&str, super::super::Build)] = &[
    ("ddlCreateSequence", create_sequence),
    ("ddlAlterSequence", alter_sequence),
    ("ddlSequenceIfNotExists", sequence_if_not_exists),
    ("ddlSequenceIfExists", sequence_if_exists),
    ("ddlSequenceAsType", sequence_as_type),
    ("ddlSequenceOptions", sequence_set_options),
    ("ddlSequenceRestart", sequence_restart),
    ("ddlSequenceOwnedBy", sequence_owned_by),
    ("ddlDropSequence", drop_sequence),
    ("ddlRenameSequence", rename_sequence),
    ("ddlCreateExtension", create_extension),
    ("ddlDropExtension", drop_extension),
];

/// A sequence statement a clause can be added to.
enum Clauses {
    Create(SequenceCreateStatement),
    Pending(PendingSequenceAlter),
    Alter(SequenceAlterStatement),
}

fn clauses(cx: &mut FunctionContext) -> NeonResult<Clauses> {
    super::receiver(cx, "a sequence", |part| match part {
        Part::Statement(Ddl::CreateSequence(create)) => Ok(Clauses::Create(create)),
        Part::PendingAlterSequence(pending) => Ok(Clauses::Pending(pending)),
        Part::Statement(Ddl::AlterSequence(alter)) => Ok(Clauses::Alter(alter)),
        other => Err(other.describe()),
    })
}

/// Apply the clause `$method`, which a create and an alter share, to either:
/// a pending alter becomes the statement it starts.
macro_rules! clause {
    ($clauses:expr, $method:ident($($arg:expr),*)) => {
        Ok(match $clauses {
            Clauses::Create(mut create) => {
                create.$method($($arg),*);
                Ddl::CreateSequence(create)
            }
            Clauses::Pending(pending) => Ddl::AlterSequence(pending.$method($($arg),*)),
            Clauses::Alter(mut alter) => {
                alter.$method($($arg),*);
                Ddl::AlterSequence(alter)
            }
        }
        .into())
    };
}

/// `ddlCreateSequence(name)`: a sequence is a relation, named as a table is.
// [spec:pgorm:req:napi.schema-sequences]
fn create_sequence(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = relation_at(cx, 0)?;
    Ok(Ddl::CreateSequence(Sequence::create(name)).into())
}

/// `ddlAlterSequence(name)`: the sequence, with no clause yet.
// [spec:pgorm:req:napi.schema-sequences]
fn alter_sequence(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = relation_at(cx, 0)?;
    Ok(Part::PendingAlterSequence(Sequence::alter(name)).into())
}

fn sequence_if_not_exists(cx: &mut FunctionContext) -> NeonResult<Node> {
    match clauses(cx)? {
        Clauses::Create(mut create) => {
            create.if_not_exists();
            Ok(Ddl::CreateSequence(create).into())
        }
        _ => refuse(cx, "IF NOT EXISTS belongs to a CREATE SEQUENCE"),
    }
}

fn sequence_if_exists(cx: &mut FunctionContext) -> NeonResult<Node> {
    match clauses(cx)? {
        Clauses::Alter(mut alter) => {
            alter.if_exists();
            Ok(Ddl::AlterSequence(alter).into())
        }
        _ => refuse(
            cx,
            "IF EXISTS belongs to an ALTER SEQUENCE with a clause to apply",
        ),
    }
}

/// `ddlSequenceAsType(sequence, type)`: the type it counts in, one of the
/// three PostgreSQL accepts.
fn sequence_as_type(cx: &mut FunctionContext) -> NeonResult<Node> {
    let clauses = clauses(cx)?;
    let kind = arg(cx, 1);
    let kind = pick(
        cx,
        kind,
        "a sequence's type",
        &[
            ("smallint", SequenceType::SmallInteger),
            ("integer", SequenceType::Integer),
            ("bigint", SequenceType::BigInteger),
        ],
    )?;
    clause!(clauses, as_type(kind))
}

/// `ddlSequenceOptions(sequence, options)`: at least one option, merged into
/// those already set, a later one for a clause replacing the earlier.
// [spec:pgorm:req:napi.schema-sequences]
fn sequence_set_options(cx: &mut FunctionContext) -> NeonResult<Node> {
    let clauses = clauses(cx)?;
    let Some(options): Option<SequenceOptions> = sequence_options(cx, 1)? else {
        return refuse(cx, "options(..) sets at least one option");
    };
    clause!(clauses, options(options))
}

/// `ddlSequenceRestart(alter, value)`: `RESTART`, at `value` when one is
/// given.
fn sequence_restart(cx: &mut FunctionContext) -> NeonResult<Node> {
    let clauses = clauses(cx)?;
    let value = arg(cx, 1);
    let value = if absent(cx, value) {
        None
    } else {
        Some(integer(cx, value, "a restart value")?)
    };
    let pending = match clauses {
        Clauses::Create(_) => return refuse(cx, "RESTART belongs to an ALTER SEQUENCE"),
        Clauses::Pending(pending) => pending,
        Clauses::Alter(mut alter) => {
            match value {
                Some(value) => alter.restart_with(value),
                None => alter.restart(),
            };
            return Ok(Ddl::AlterSequence(alter).into());
        }
    };
    Ok(Ddl::AlterSequence(match value {
        Some(value) => pending.restart_with(value),
        None => pending.restart(),
    })
    .into())
}

/// `ddlSequenceOwnedBy(sequence, table, column)`, or with a `null` table
/// `OWNED BY NONE`, which releases it.
// [spec:pgorm:req:napi.schema-sequences]
fn sequence_owned_by(cx: &mut FunctionContext) -> NeonResult<Node> {
    let clauses = clauses(cx)?;
    let table = arg(cx, 1);
    if table.is_a::<JsNull, _>(cx) {
        return clause!(clauses, owned_by_none());
    }
    let table = relation_at(cx, 1)?;
    let column = name_at(cx, 2)?;
    clause!(clauses, owned_by(table, column))
}

/// `ddlDropSequence(names, { ifExists, behavior })`.
fn drop_sequence(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (first, rest) = relations_at(cx, 0)?;
    let options = object(cx, 1, &["ifExists", "behavior"])?;
    let mut drop = Sequence::drop(first);
    for name in rest {
        drop.name(name);
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
    Ok(Ddl::DropSequence(drop).into())
}

fn rename_sequence(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = relation_at(cx, 0)?;
    let to = name_at(cx, 1)?;
    Ok(Ddl::RenameSequence(Sequence::rename(name, to)).into())
}

/// `ddlCreateExtension(name, { ifNotExists, schema, version, cascade })`: the
/// name and schema quoted, the version a literal.
// [spec:pgorm:req:napi.schema-sequences]
fn create_extension(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = name_at(cx, 0)?;
    let options = object(cx, 1, &["ifNotExists", "schema", "version", "cascade"])?;
    let mut create = Extension::create(name);
    if flag(cx, options, "ifNotExists")? {
        create.if_not_exists();
    }
    if let Some(schema) = name_option(cx, options, "schema")? {
        create.schema(schema);
    }
    if let Some(version) = get(cx, options, "version")? {
        create.version(text(cx, version, "an extension's version")?);
    }
    if flag(cx, options, "cascade")? {
        create.cascade();
    }
    Ok(Ddl::CreateExtension(create).into())
}

fn drop_extension(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = name_at(cx, 0)?;
    let options = object(cx, 1, &["ifExists", "behavior"])?;
    let mut drop = Extension::drop(name);
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
    Ok(Ddl::DropExtension(drop).into())
}
