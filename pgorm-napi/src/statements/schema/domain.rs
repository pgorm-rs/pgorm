//! Domains — created, altered over pgorm-query's typestate and dropped — and
//! `VALUE`, the value a domain's `CHECK` tests. A domain is a type, so it is
//! named as one is: a string, or a `TypeName` whose schema qualifies it.

use neon::prelude::*;
use pgorm::pgorm_query::{
    ConstraintDrop, Domain, DomainCheck, DomainCreateStatement, DropBehavior, Expr,
    PendingDomainAlter,
};

use super::{
    super::{
        Node,
        args::{arg, expression, name_at, operand_at, refuse},
        data_type::column_type,
    },
    Ddl, Part,
    options::{BEHAVIOR, choice, collation, flag, name_option, object, type_ref},
};

pub(super) const EXPORTS: &[(&str, super::super::Build)] = &[
    ("ddlCreateDomain", create_domain),
    ("ddlDomainCollate", domain_collate),
    ("ddlDomainDefault", domain_default),
    ("ddlDomainNotNull", domain_not_null),
    ("ddlDomainNull", domain_null),
    ("ddlDomainCheck", domain_check),
    ("ddlAlterDomain", alter_domain),
    ("ddlDomainSetDefault", domain_set_default),
    ("ddlDomainDropDefault", domain_drop_default),
    ("ddlDomainSetNotNull", domain_set_not_null),
    ("ddlDomainDropNotNull", domain_drop_not_null),
    ("ddlDomainAddCheck", domain_add_check),
    ("ddlDomainDropConstraint", domain_drop_constraint),
    ("ddlDomainRenameConstraint", domain_rename_constraint),
    ("ddlDomainValidateConstraint", domain_validate_constraint),
    ("ddlDomainRenameTo", domain_rename_to),
    ("ddlDropDomain", drop_domain),
    ("exprDomainValue", domain_value),
];

fn created(cx: &mut FunctionContext) -> NeonResult<DomainCreateStatement> {
    super::receiver(cx, "a CREATE DOMAIN", |part| match part {
        Part::Statement(Ddl::CreateDomain(create)) => Ok(create),
        other => Err(other.describe()),
    })
}

/// Apply `change` to the receiver, a `CREATE DOMAIN`, giving the new one.
fn with(
    cx: &mut FunctionContext,
    change: impl FnOnce(&mut FunctionContext, &mut DomainCreateStatement) -> NeonResult<()>,
) -> NeonResult<Node> {
    let mut statement = created(cx)?;
    change(cx, &mut statement)?;
    Ok(Ddl::CreateDomain(statement).into())
}

/// A domain's `CHECK` over the expression at `index`, named as the options
/// object after it says, and whether it is added `NOT VALID` where `extra`
/// lets the options say so.
// [spec:pgorm:req:napi.schema-domains]
fn check(
    cx: &mut FunctionContext,
    index: usize,
    extra: &[&str],
) -> NeonResult<(DomainCheck, bool)> {
    let condition = arg(cx, index);
    let condition = expression(cx, condition)?;
    let mut known = vec!["name"];
    known.extend_from_slice(extra);
    let options = object(cx, index + 1, &known)?;
    let mut check = DomainCheck::new(condition);
    if let Some(name) = name_option(cx, options, "name")? {
        check = check.name(name);
    }
    let not_valid = flag(cx, options, "notValid")?;
    Ok((check, not_valid))
}

/// `ddlCreateDomain(name, base)`: the domain and the type it is over, which
/// the grammar requires both of.
// [spec:pgorm:req:napi.schema-domains]
fn create_domain(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = arg(cx, 0);
    let name = type_ref(cx, name)?;
    let base = arg(cx, 1);
    let base = column_type(cx, base)?;
    Ok(Ddl::CreateDomain(Domain::create(name, base)).into())
}

/// `ddlDomainCollate(create, collation)`: a name, or `{ name, schema }`.
fn domain_collate(cx: &mut FunctionContext) -> NeonResult<Node> {
    with(cx, |cx, create| {
        let named = arg(cx, 1);
        let named = collation(cx, named)?;
        create.collate(named);
        Ok(())
    })
}

/// `ddlDomainDefault(create, value)`: an expression, or a value written as
/// pgorm-query's literal; a later default replaces it.
// [spec:pgorm:req:napi.schema-domains]
fn domain_default(cx: &mut FunctionContext) -> NeonResult<Node> {
    with(cx, |cx, create| {
        let value = operand_at(cx, 1)?;
        create.default(value);
        Ok(())
    })
}

/// `NOT NULL` and `NULL` share one slot, the last call winning, as
/// PostgreSQL refuses the two together.
fn domain_not_null(cx: &mut FunctionContext) -> NeonResult<Node> {
    with(cx, |_, create| {
        create.not_null();
        Ok(())
    })
}

fn domain_null(cx: &mut FunctionContext) -> NeonResult<Node> {
    with(cx, |_, create| {
        create.null();
        Ok(())
    })
}

/// `ddlDomainCheck(create, condition, { name })`: a check appended, which
/// takes no `notValid`, PostgreSQL having no `NOT VALID` at `CREATE DOMAIN`.
// [spec:pgorm:req:napi.schema-domains]
fn domain_check(cx: &mut FunctionContext) -> NeonResult<Node> {
    let (check, _) = check(cx, 1, &[])?;
    with(cx, |_, create| {
        create.check(check);
        Ok(())
    })
}

/// `ddlAlterDomain(name)`: the domain, with no action yet.
// [spec:pgorm:req:napi.schema-domains]
fn alter_domain(cx: &mut FunctionContext) -> NeonResult<Node> {
    let name = arg(cx, 0);
    let name = type_ref(cx, name)?;
    Ok(Part::PendingAlterDomain(Domain::alter(name)).into())
}

fn pending(cx: &mut FunctionContext) -> NeonResult<PendingDomainAlter> {
    super::receiver(cx, "an ALTER DOMAIN", |part| match part {
        Part::PendingAlterDomain(pending) => Ok(pending),
        other => Err(other.describe()),
    })
}

fn domain_set_default(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
    let value = operand_at(cx, 1)?;
    Ok(Ddl::AlterDomain(pending.set_default(value)).into())
}

fn domain_drop_default(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
    Ok(Ddl::AlterDomain(pending.drop_default()).into())
}

fn domain_set_not_null(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
    Ok(Ddl::AlterDomain(pending.set_not_null()).into())
}

fn domain_drop_not_null(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
    Ok(Ddl::AlterDomain(pending.drop_not_null()).into())
}

/// `ddlDomainAddCheck(alter, condition, { name, notValid })`: `notValid`
/// leaves the values columns of the domain already hold unchecked until the
/// constraint is validated.
// [spec:pgorm:req:napi.schema-domains]
fn domain_add_check(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
    let (check, not_valid) = check(cx, 1, &["notValid"])?;
    let statement = if not_valid {
        pending.add_check(check.not_valid())
    } else {
        pending.add_check(check)
    };
    Ok(Ddl::AlterDomain(statement).into())
}

/// `ddlDomainDropConstraint(alter, name, { ifExists, behavior })`.
// [spec:pgorm:req:napi.schema-domains]
fn domain_drop_constraint(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
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
    Ok(Ddl::AlterDomain(pending.drop_constraint(drop)).into())
}

fn domain_rename_constraint(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
    let from = name_at(cx, 1)?;
    let to = name_at(cx, 2)?;
    Ok(Ddl::AlterDomain(pending.rename_constraint(from, to)).into())
}

fn domain_validate_constraint(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
    let name = name_at(cx, 1)?;
    Ok(Ddl::AlterDomain(pending.validate_constraint(name)).into())
}

/// `ddlDomainRenameTo(alter, name)`: the new name is bare, a rename leaving
/// the domain in its schema.
fn domain_rename_to(cx: &mut FunctionContext) -> NeonResult<Node> {
    let pending = pending(cx)?;
    let name = name_at(cx, 1)?;
    Ok(Ddl::AlterDomain(pending.rename_to(name)).into())
}

/// `ddlDropDomain(names, { ifExists, behavior })`: one domain, or a
/// non-empty array of them.
// [spec:pgorm:req:napi.schema-domains]
fn drop_domain(cx: &mut FunctionContext) -> NeonResult<Node> {
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
        return refuse(cx, "a drop names at least one domain");
    };
    let options = object(cx, 1, &["ifExists", "behavior"])?;
    let mut drop = Domain::drop(first);
    refs.for_each(|name| {
        drop.name(name);
    });
    if flag(cx, options, "ifExists")? {
        drop.if_exists();
    }
    match choice(cx, options, "behavior", BEHAVIOR)? {
        Some(DropBehavior::Cascade) => drop.cascade(),
        Some(DropBehavior::Restrict) => drop.restrict(),
        None => &mut drop,
    };
    Ok(Ddl::DropDomain(drop).into())
}

/// `exprDomainValue()`: `VALUE`, which PostgreSQL reads inside a domain's
/// `CHECK` as the value tested and anywhere else as a column named `value`.
// [spec:pgorm:req:napi.schema-domains]
fn domain_value(_: &mut FunctionContext) -> NeonResult<Node> {
    Ok(Node::Expr(Expr::domain_value().into()))
}
