use super::{Enums, unsupported};
use crate::Error;
use pg_query::NodeEnum;
use pg_query::protobuf::TypeName;
use pgorm_query::{Alias, ColumnType, RcOrArc, SeaRc, StringLen};

/// A column's type together with the auto-increment fact the `serial` family
/// carries in its spelling rather than in a constraint.
pub(super) struct ColumnKind {
    pub(super) col_type: ColumnType,
    pub(super) auto_increment: bool,
}

/// Read a parsed `TypeName` as the `ColumnType` that renders it.
///
/// `context` names the column for the error message; `at` is the 1-based
/// statement number.
// [spec:pgorm:sem:codegen.ddl.types]
pub(super) fn column_kind(
    type_name: &TypeName,
    enums: &Enums,
    context: &str,
    at: usize,
) -> Result<ColumnKind, Error> {
    let names = idents(&type_name.names)
        .ok_or_else(|| unsupported(format!("a computed type name on {context}"), at))?;
    let modifiers = modifiers(type_name, context, at)?;
    let kind = named_type(&names, &modifiers, enums, context, at)?;
    match type_name.array_bounds.as_slice() {
        [] => Ok(kind),
        [bound] => {
            let unsized_bound =
                matches!(bound.node, Some(NodeEnum::Integer(ref i)) if i.ival == -1);
            if !unsized_bound {
                return Err(unsupported(format!("a sized array on {context}"), at));
            }
            if kind.auto_increment {
                return Err(unsupported(format!("an array of serial on {context}"), at));
            }
            Ok(ColumnKind {
                col_type: ColumnType::Array(RcOrArc::new(kind.col_type)),
                auto_increment: false,
            })
        }
        _ => Err(unsupported(
            format!("a multi-dimensional array on {context}"),
            at,
        )),
    }
}

/// The type name's own arguments — `varchar(255)`'s 255, `numeric(10, 2)`'s
/// 10 and 2. Anything that is not a plain non-negative integer is out of the
/// bridge's reach.
fn modifiers(type_name: &TypeName, context: &str, at: usize) -> Result<Vec<u32>, Error> {
    let mut modifiers = Vec::with_capacity(type_name.typmods.len());
    for node in &type_name.typmods {
        let value = match &node.node {
            Some(NodeEnum::AConst(constant)) => match &constant.val {
                Some(pg_query::protobuf::a_const::Val::Ival(int)) => u32::try_from(int.ival).ok(),
                _ => None,
            },
            _ => None,
        };
        match value {
            Some(value) => modifiers.push(value),
            None => {
                return Err(unsupported(
                    format!("a non-integer type modifier on {context}"),
                    at,
                ));
            }
        }
    }
    Ok(modifiers)
}

/// The reverse of the `ColumnType` → Postgres spelling contract, read over the
/// names the grammar produces: keyword spellings arrive qualified as
/// `pg_catalog.<name>`, everything else bare.
// [spec:pgorm:sem:codegen.ddl.types]
fn named_type(
    names: &[String],
    modifiers: &[u32],
    enums: &Enums,
    context: &str,
    at: usize,
) -> Result<ColumnKind, Error> {
    let (catalog, name) = match names {
        [name] => (false, name),
        [schema, name] if schema == "pg_catalog" => (true, name),
        [_, name] => (false, name),
        _ => return Err(unsupported(format!("a type name on {context}"), at)),
    };
    if !catalog {
        if let Some(variants) = enums.get(name.as_str()) {
            return Ok(plain(ColumnType::Enum {
                name: SeaRc::new(Alias::new(name.as_str())),
                variants: variants
                    .iter()
                    .map(|variant| SeaRc::new(Alias::new(variant.as_str())))
                    .collect(),
            }));
        }
    }
    let col_type = match (name.as_str(), modifiers) {
        ("serial" | "serial4", []) => return Ok(serial(ColumnType::Integer)),
        ("bigserial" | "serial8", []) => return Ok(serial(ColumnType::BigInteger)),
        ("smallserial" | "serial2", []) => return Ok(serial(ColumnType::SmallInteger)),
        ("bpchar", []) => ColumnType::Char(None),
        ("bpchar", [length]) => ColumnType::Char(Some(*length)),
        ("varchar", []) => ColumnType::String(StringLen::None),
        ("varchar", [length]) => ColumnType::String(StringLen::N(*length)),
        ("text", []) => ColumnType::Text,
        ("int2" | "smallint", []) => ColumnType::SmallInteger,
        ("int4" | "int" | "integer", []) => ColumnType::Integer,
        ("int8" | "bigint", []) => ColumnType::BigInteger,
        ("float4" | "real", []) => ColumnType::Float,
        ("float8", []) => ColumnType::Double,
        ("numeric" | "decimal", []) => ColumnType::Decimal(None),
        ("numeric" | "decimal", [precision]) => ColumnType::Decimal(Some((*precision, 0))),
        ("numeric" | "decimal", [precision, scale]) => {
            ColumnType::Decimal(Some((*precision, *scale)))
        }
        ("timestamp", []) => ColumnType::DateTime,
        ("timestamptz", []) => ColumnType::TimestampWithTimeZone,
        ("time", []) => ColumnType::Time,
        ("date", []) => ColumnType::Date,
        ("interval", []) => ColumnType::Interval(None, None),
        ("bool" | "boolean", []) => ColumnType::Boolean,
        ("money", []) => ColumnType::Money(None),
        ("bytea", []) => ColumnType::Blob,
        ("bit", []) => ColumnType::Bit(None),
        ("bit", [length]) => ColumnType::Bit(Some(*length)),
        ("varbit", [length]) => ColumnType::VarBit(*length),
        ("json", []) => ColumnType::Json,
        ("jsonb", []) => ColumnType::JsonBinary,
        ("uuid", []) => ColumnType::Uuid,
        ("inet", []) => ColumnType::Inet,
        ("cidr", []) => ColumnType::Cidr,
        ("macaddr", []) => ColumnType::MacAddr,
        ("ltree", []) => ColumnType::LTree,
        ("vector", []) => ColumnType::Vector(None),
        ("vector", [size]) => ColumnType::Vector(Some(*size)),
        ("varbit", []) => {
            return Err(unsupported(
                format!("`varbit` without a length on {context}"),
                at,
            ));
        }
        _ if !modifiers.is_empty() => {
            return Err(unsupported(
                format!("`{name}` with a type modifier on {context}"),
                at,
            ));
        }
        _ => return Err(unsupported(format!("type `{name}` on {context}"), at)),
    };
    Ok(plain(col_type))
}

fn plain(col_type: ColumnType) -> ColumnKind {
    ColumnKind {
        col_type,
        auto_increment: false,
    }
}

fn serial(col_type: ColumnType) -> ColumnKind {
    ColumnKind {
        col_type,
        auto_increment: true,
    }
}

/// The `String` nodes of a name list, or `None` when the list holds anything
/// else.
pub(super) fn idents(nodes: &[pg_query::protobuf::Node]) -> Option<Vec<String>> {
    nodes
        .iter()
        .map(|node| match &node.node {
            Some(NodeEnum::String(text)) if !text.sval.is_empty() => Some(text.sval.clone()),
            _ => None,
        })
        .collect()
}
