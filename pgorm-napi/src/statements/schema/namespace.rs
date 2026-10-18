//! Schemas — created, dropped and renamed — each named by one string: a
//! schema has no schema of its own, so a `Table` is no schema name.

use neon::prelude::*;
use pgorm::pgorm_query::{DropBehavior, Name, Schema};

use super::{
    super::{
        Node,
        args::{arg, name, name_at, refuse},
    },
    Ddl,
    options::{BEHAVIOR, choice, flag, object},
};

pub(super) const EXPORTS: &[(&str, super::super::Build)] = &[
    ("ddlCreateSchema", create_schema),
    ("ddlDropSchema", drop_schema),
    ("ddlRenameSchema", rename_schema),
];

/// The schemas a drop names: one name, or a non-empty array of them, the
/// first apart as pgorm-query takes it.
fn schemas_at(cx: &mut FunctionContext, index: usize) -> NeonResult<(Name, Vec<Name>)> {
    let value = arg(cx, index);
    let Ok(array) = value.downcast::<JsArray, _>(cx) else {
        return Ok((name(cx, value)?, Vec::new()));
    };
    let mut all = Vec::new();
    for item in array.to_vec(cx)? {
        all.push(name(cx, item)?);
    }
    if all.is_empty() {
        return refuse(cx, "a drop names at least one schema");
    }
    let first = all.remove(0);
    Ok((first, all))
}

/// `ddlCreateSchema(name, { ifNotExists })`.
// [spec:pgorm:req:napi.schema-schemas]
fn create_schema(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = name_at(cx, 0)?;
    let options = object(cx, 1, &["ifNotExists"])?;
    let mut create = Schema::create(name);
    if flag(cx, options, "ifNotExists")? {
        create.if_not_exists();
    }
    Ok(Ddl::CreateSchema(create).into())
}

/// `ddlDropSchema(names, { ifExists, behavior })`.
// [spec:pgorm:req:napi.schema-schemas]
fn drop_schema(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (first, rest) = schemas_at(cx, 0)?;
    let options = object(cx, 1, &["ifExists", "behavior"])?;
    let if_exists = flag(cx, options, "ifExists")?;
    let behavior = choice(cx, options, "behavior", BEHAVIOR)?;
    let mut drop = Schema::drop(first);
    rest.into_iter().for_each(|name| {
        drop.name(name);
    });
    if if_exists {
        drop.if_exists();
    }
    match behavior {
        Some(DropBehavior::Cascade) => drop.cascade(),
        Some(DropBehavior::Restrict) => drop.restrict(),
        None => &mut drop,
    };
    Ok(Ddl::DropSchema(drop).into())
}

/// `ddlRenameSchema(name, newName)`, which PostgreSQL gives no `IF EXISTS`.
// [spec:pgorm:req:napi.schema-schemas]
fn rename_schema(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = name_at(cx, 0)?;
    let to = name_at(cx, 1)?;
    Ok(Ddl::RenameSchema(Schema::rename(name, to)).into())
}
