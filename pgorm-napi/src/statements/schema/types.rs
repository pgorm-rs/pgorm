//! `CREATE TYPE` — a shell type, an enumeration, a composite or a range —
//! `ALTER TYPE` over pgorm-query's typestate, and `DROP TYPE`. What a type
//! is, is one slot: choosing a kind replaces the one before, as pgorm-query's
//! builder does.

use neon::prelude::*;
use pgorm::pgorm_query::{
    ColumnType, DropBehavior,
    extension::{
        CompositeAlterStatement, PendingTypeAlter, RangeDefinition, Type, TypeCreateStatement,
    },
};

use super::{
    super::{
        Node,
        args::{arg, name_at, refuse, this},
        data_type::column_type,
    },
    Ddl, Part,
    options::{
        BEHAVIOR, choice, collation_option, flag, get, label, name_option, object, pick, type_ref,
    },
};

pub(super) const EXPORTS: &[(&str, super::super::Build)] = &[
    ("ddlCreateType", create_type),
    ("ddlTypeAsEnum", type_as_enum),
    ("ddlTypeValues", type_values),
    ("ddlTypeAsComposite", type_as_composite),
    ("ddlTypeAttribute", type_attribute),
    ("ddlTypeAsRange", type_as_range),
    ("ddlAlterType", alter_type),
    ("ddlTypeAddValue", type_add_value),
    ("ddlTypeRenameTo", type_rename_to),
    ("ddlTypeRenameValue", type_rename_value),
    ("ddlTypeRenameAttribute", type_rename_attribute),
    ("ddlCompositeAdd", composite_add),
    ("ddlCompositeDrop", composite_drop),
    ("ddlCompositeAlter", composite_alter),
    ("ddlTypeBehavior", type_behavior),
    ("ddlDropType", drop_type),
];

fn created(cx: &mut FunctionContext) -> NeonResult<TypeCreateStatement> {
    super::receiver(cx, "a CREATE TYPE", |part| match part {
        Part::Statement(Ddl::CreateType(create)) => Ok(create),
        other => Err(other.describe()),
    })
}

/// The type at `index`: a `DataType`, a built-in type's name, a `TypeName`
/// or a created range.
fn type_at(cx: &mut FunctionContext, index: usize) -> NeonResult<ColumnType> {
    let value = arg(cx, index);
    column_type(cx, value)
}

/// `ddlCreateType(name)`: a shell type until a kind is chosen.
// [spec:pgorm:req:napi.schema-types]
fn create_type(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = arg(cx, 0);
    let name = type_ref(cx, name)?;
    Ok(Ddl::CreateType(Type::create(name)).into())
}

fn type_as_enum(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut statement = created(cx)?;
    statement.as_enum();
    Ok(Ddl::CreateType(statement).into())
}

/// `ddlTypeValues(type, labels)`: labels appended to an enumeration, which
/// the type becomes. A label is data, written as a literal.
// [spec:pgorm:req:napi.schema-types]
fn type_values(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut statement = created(cx)?;
    let labels = arg(cx, 1);
    let Ok(labels) = labels.downcast::<JsArray, _>(cx) else {
        return cx.throw_type_error("an enumeration's labels are an array of strings");
    };
    let mut values = Vec::new();
    for item in labels.to_vec(cx)? {
        values.push(label(cx, item)?);
    }
    statement.values(values);
    Ok(Ddl::CreateType(statement).into())
}

fn type_as_composite(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut statement = created(cx)?;
    statement.as_composite();
    Ok(Ddl::CreateType(statement).into())
}

/// `ddlTypeAttribute(type, name, type, { collation })`: an attribute appended
/// to a composite, which the type becomes.
// [spec:pgorm:req:napi.schema-types]
fn type_attribute(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut statement = created(cx)?;
    let name = name_at(cx, 1)?;
    let kind = type_at(cx, 2)?;
    let options = object(cx, 3, &["collation"])?;
    match collation_option(cx, options)? {
        Some(collation) => statement.attribute_collated(name, kind, collation),
        None => statement.attribute(name, kind),
    };
    Ok(Ddl::CreateType(statement).into())
}

/// `ddlTypeAsRange(type, subtype, options)`: a range over `subtype`, the one
/// option PostgreSQL requires, with the operator class, collation,
/// difference function and multirange name the options give.
// [spec:pgorm:req:napi.schema-types]
fn type_as_range(cx: &mut FunctionContext) -> NeonResult<Node> {
    let mut statement = created(cx)?;
    let subtype = type_at(cx, 1)?;
    let options = object(
        cx,
        2,
        &[
            "subtypeOpclass",
            "collation",
            "subtypeDiff",
            "multirangeTypeName",
        ],
    )?;
    let mut range = RangeDefinition::new(subtype);
    if let Some(opclass) = name_option(cx, options, "subtypeOpclass")? {
        range = range.subtype_opclass(opclass);
    }
    if let Some(collation) = collation_option(cx, options)? {
        range = range.collation(collation);
    }
    if let Some(function) = name_option(cx, options, "subtypeDiff")? {
        range = range.subtype_diff(function);
    }
    if let Some(name) = get(cx, options, "multirangeTypeName")? {
        range = range.multirange_type_name(type_ref(cx, name)?);
    }
    statement.as_range(range);
    Ok(Ddl::CreateType(statement).into())
}

/// `ddlAlterType(name)`: the type, with no change yet.
// [spec:pgorm:req:napi.schema-types]
fn alter_type(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = arg(cx, 0);
    let name = type_ref(cx, name)?;
    Ok(Part::PendingAlterType(Type::alter(name)).into())
}

fn pending(cx: &mut FunctionContext) -> NeonResult<PendingTypeAlter> {
    super::receiver(cx, "an ALTER TYPE", |part| match part {
        Part::PendingAlterType(pending) => Ok(pending),
        other => Err(other.describe()),
    })
}

/// `ddlTypeAddValue(alter, label, { before, after })`: a label added to an
/// enumeration, at its end or beside one, never both.
// [spec:pgorm:req:napi.schema-types]
fn type_add_value(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
    let value = arg(cx, 1);
    let value = label(cx, value)?;
    let options = object(cx, 2, &["before", "after"])?;
    let before = get(cx, options, "before")?;
    let after = get(cx, options, "after")?;
    let statement = pending.add_value(value);
    let statement = match (before, after) {
        (Some(_), Some(_)) => {
            return refuse(
                cx,
                "a label is added before one label or after one, not both",
            );
        }
        (Some(before), None) => statement.before(label(cx, before)?),
        (None, Some(after)) => statement.after(label(cx, after)?),
        (None, None) => statement,
    };
    Ok(Ddl::AlterType(statement).into())
}

/// `ddlTypeRenameTo(alter, name)`: the new name is bare, a rename leaving the
/// type in its schema.
fn type_rename_to(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
    let name = name_at(cx, 1)?;
    Ok(Ddl::AlterType(pending.rename_to(name)).into())
}

fn type_rename_value(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
    let from = arg(cx, 1);
    let from = label(cx, from)?;
    let to = arg(cx, 2);
    let to = label(cx, to)?;
    Ok(Ddl::AlterType(pending.rename_value(from, to)).into())
}

/// `ddlTypeRenameAttribute(alter, name, newName)`: a statement of its own,
/// PostgreSQL taking `RENAME` as an `ALTER TYPE`'s sole action.
// [spec:pgorm:req:napi.schema-types]
fn type_rename_attribute(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
    let from = name_at(cx, 1)?;
    let to = name_at(cx, 2)?;
    Ok(Ddl::RenameAttribute(pending.rename_attribute(from, to)).into())
}

/// A composite's alteration before or after its first change.
enum Composite {
    Pending(PendingTypeAlter),
    Statement(CompositeAlterStatement),
}

fn composite(cx: &mut FunctionContext) -> NeonResult<Composite> {
    super::receiver(cx, "an ALTER TYPE", |part| match part {
        Part::PendingAlterType(pending) => Ok(Composite::Pending(pending)),
        Part::Statement(Ddl::AlterComposite(statement)) => Ok(Composite::Statement(statement)),
        other => Err(other.describe()),
    })
}

/// Apply the pgorm-query change `$method` to a composite's alteration, which
/// on a pending alter consumes it and on a statement appends.
macro_rules! change {
    ($composite:expr, $method:ident($($arg:expr),*)) => {
        Ok(Ddl::AlterComposite(match $composite {
            Composite::Pending(pending) => pending.$method($($arg),*),
            Composite::Statement(statement) => statement.$method($($arg),*),
        })
        .into())
    };
}

/// `ddlCompositeAdd(alter, name, type, { collation })`.
// [spec:pgorm:req:napi.schema-types]
fn composite_add(cx: &mut FunctionContext) -> NeonResult<Node> {
    let composite = composite(cx)?;
    let name = name_at(cx, 1)?;
    let kind = type_at(cx, 2)?;
    let options = object(cx, 3, &["collation"])?;
    match collation_option(cx, options)? {
        Some(collation) => change!(composite, add_attribute_collated(name, kind, collation)),
        None => change!(composite, add_attribute(name, kind)),
    }
}

fn composite_drop(cx: &mut FunctionContext) -> NeonResult<Node> {
    let composite = composite(cx)?;
    let name = name_at(cx, 1)?;
    let options = object(cx, 2, &["ifExists"])?;
    if flag(cx, options, "ifExists")? {
        change!(composite, drop_attribute_if_exists(name))
    } else {
        change!(composite, drop_attribute(name))
    }
}

fn composite_alter(cx: &mut FunctionContext) -> NeonResult<Node> {
    let composite = composite(cx)?;
    let name = name_at(cx, 1)?;
    let kind = type_at(cx, 2)?;
    let options = object(cx, 3, &["collation"])?;
    match collation_option(cx, options)? {
        Some(collation) => change!(composite, alter_attribute_collated(name, kind, collation)),
        None => change!(composite, alter_attribute(name, kind)),
    }
}

/// `ddlTypeBehavior(alter, behavior)`: `CASCADE` or `RESTRICT` after a
/// composite's changes or an attribute's rename, the last call winning.
// [spec:pgorm:req:napi.schema-types]
fn type_behavior(cx: &mut FunctionContext) -> NeonResult<Node> {
    let receiver = this(cx, 0)?;
    let behavior = arg(cx, 1);
    let behavior = pick(cx, behavior, "a behavior", BEHAVIOR)?;
    let Node::Schema(part) = receiver else {
        return refuse(cx, "expected an ALTER TYPE that changes attributes");
    };
    let ddl = match (*part, behavior) {
        (Part::Statement(Ddl::AlterComposite(statement)), DropBehavior::Cascade) => {
            Ddl::AlterComposite(statement.cascade())
        }
        (Part::Statement(Ddl::AlterComposite(statement)), DropBehavior::Restrict) => {
            Ddl::AlterComposite(statement.restrict())
        }
        (Part::Statement(Ddl::RenameAttribute(statement)), DropBehavior::Cascade) => {
            Ddl::RenameAttribute(statement.cascade())
        }
        (Part::Statement(Ddl::RenameAttribute(statement)), DropBehavior::Restrict) => {
            Ddl::RenameAttribute(statement.restrict())
        }
        _ => return refuse(cx, "expected an ALTER TYPE that changes attributes"),
    };
    Ok(ddl.into())
}

/// `ddlDropType(names, { ifExists, behavior })`.
// [spec:pgorm:req:napi.schema-types]
fn drop_type(cx: &mut FunctionContext) -> NeonResult<Node> {
    let names = arg(cx, 0);
    let names = match names.downcast::<JsArray, _>(cx) {
        Ok(array) => array.to_vec(cx)?,
        Err(_) => vec![names],
    };
    let mut refs = Vec::new();
    for name in names {
        refs.push(type_ref(cx, name)?);
    }
    let mut refs = refs.into_iter();
    let Some(first) = refs.next() else {
        return refuse(cx, "a drop names at least one type");
    };
    let options = object(cx, 1, &["ifExists", "behavior"])?;
    let mut drop = Type::drop(first);
    drop.names(refs);
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
    Ok(Ddl::DropType(drop).into())
}
