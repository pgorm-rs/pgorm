use super::{table, types, unresolved, unsupported};
use crate::{Error, TableIdent};
use pg_query::NodeEnum;
use pg_query::protobuf::{
    CommentStmt, CreateEnumStmt, IndexStmt, ObjectType, SortByDir, SortByNulls,
};
use pgorm_query::{Name, TableKey, Unique};

/// A `CREATE TYPE ... AS ENUM` as the full identity — schema and name — and
/// values a column of that type carries into `ColumnType::Enum`.
// [spec:pgorm:sem:codegen.ddl.objects+6]
pub(super) fn enum_type(
    stmt: &CreateEnumStmt,
    at: usize,
) -> Result<(super::EnumIdentity, Vec<String>), Error> {
    let names = types::idents(&stmt.type_name)
        .ok_or_else(|| unsupported("a computed type name in CREATE TYPE", at))?;
    let identity = match names.as_slice() {
        [name] => (None, name.clone()),
        [schema, name] => (Some(schema.clone()), name.clone()),
        [] => return Err(unresolved("CREATE TYPE without a type name", at)),
        _ => {
            return Err(unsupported(
                "a catalog-qualified type name in CREATE TYPE",
                at,
            ));
        }
    };
    let values = types::idents(&stmt.vals)
        .ok_or_else(|| unsupported("a computed value in CREATE TYPE", at))?;
    Ok((identity, values))
}

/// A `CREATE INDEX`, and the table it belongs to.
pub(super) struct ParsedIndex {
    pub(super) table: TableIdent,
    /// A unique index as the unique key it enforces — `None` for an index
    /// that states no entity fact, which has no place inside a
    /// `CREATE TABLE`.
    pub(super) constraint: Option<TableKey<Unique>>,
}

// [spec:pgorm:sem:codegen.ddl.objects+6]
// [spec:pgorm:req:codegen.ddl.unsupported+10]
pub(super) fn index(stmt: &IndexStmt, at: usize) -> Result<ParsedIndex, Error> {
    let table = match stmt.relation.as_ref() {
        Some(relation) if !relation.relname.is_empty() => TableIdent {
            table: relation.relname.clone(),
            schema: table::schema_of(relation),
        },
        _ => return Err(unresolved("CREATE INDEX without a table name", at)),
    };
    let name = if stmt.idxname.is_empty() {
        format!("index on `{table}`")
    } else {
        format!("index `{}`", stmt.idxname)
    };
    let on = |what: &str| unsupported(format!("{what} on {name}"), at);
    if stmt.concurrent {
        return Err(on("CONCURRENTLY"));
    }
    if stmt.where_clause.is_some() {
        return Err(on("a WHERE clause"));
    }
    if !stmt.index_including_params.is_empty() {
        return Err(on("an INCLUDE clause"));
    }
    if !stmt.options.is_empty() {
        return Err(on("a WITH storage option"));
    }
    if !stmt.table_space.is_empty() {
        return Err(on("a TABLESPACE clause"));
    }
    if !stmt.exclude_op_names.is_empty() {
        return Err(on("an exclusion operator"));
    }

    let mut columns = Vec::with_capacity(stmt.index_params.len());
    for node in &stmt.index_params {
        let Some(NodeEnum::IndexElem(element)) = &node.node else {
            return Err(on("an index element"));
        };
        if element.name.is_empty() || element.expr.is_some() {
            return Err(on("an expression column"));
        }
        if !element.collation.is_empty() {
            return Err(on("a COLLATE clause"));
        }
        if !element.opclass.is_empty() || !element.opclassopts.is_empty() {
            return Err(on("an operator class"));
        }
        if element.nulls_ordering != SortByNulls::SortbyNullsDefault as i32 {
            return Err(on("a NULLS FIRST or NULLS LAST clause"));
        }
        let descending = match SortByDir::try_from(element.ordering) {
            Ok(SortByDir::SortbyDefault | SortByDir::SortbyAsc) => false,
            Ok(SortByDir::SortbyDesc) => true,
            _ => return Err(on("an index column ordering")),
        };
        columns.push((Name::runtime(element.name.as_str()), descending));
    }
    if columns.is_empty() {
        return Err(on("an index over no columns"));
    }
    if !stmt.unique {
        return Ok(ParsedIndex {
            table,
            constraint: None,
        });
    }

    // A unique index is carried as the table constraint that enforces the
    // same uniqueness, and a table constraint's key is plain column names
    // under the default btree: a descending key column or another access
    // method has no spelling there, so each is named rather than dropped.
    let on_unique = |what: &str| unsupported(format!("{what} on unique {name}"), at);
    if !matches!(stmt.access_method.as_str(), "" | "btree") {
        return Err(on_unique("an access method other than btree"));
    }
    if columns.iter().any(|(_, descending)| *descending) {
        return Err(on_unique("a DESC column"));
    }
    let mut columns = columns.into_iter().map(|(column, _)| column);
    let Some(first) = columns.next() else {
        return Err(on("an index over no columns"));
    };
    let mut constraint = TableKey::new(first).cols(columns);
    if stmt.nulls_not_distinct {
        constraint = constraint.nulls_not_distinct();
    }
    if !stmt.idxname.is_empty() {
        constraint = constraint.name(Name::runtime(stmt.idxname.as_str()));
    }
    Ok(ParsedIndex {
        table,
        constraint: Some(constraint),
    })
}

/// A `COMMENT ON` the bridge can attach to a table or one of its columns.
pub(super) enum ParsedComment {
    Table {
        table: TableIdent,
        text: String,
    },
    Column {
        table: TableIdent,
        column: String,
        text: String,
    },
}

impl ParsedComment {
    pub(super) fn table(&self) -> &TableIdent {
        match self {
            Self::Table { table, .. } | Self::Column { table, .. } => table,
        }
    }
}

// [spec:pgorm:sem:codegen.ddl.objects+6]
pub(super) fn comment(stmt: &CommentStmt, at: usize) -> Result<ParsedComment, Error> {
    let kind = match ObjectType::try_from(stmt.objtype) {
        Ok(kind @ (ObjectType::ObjectTable | ObjectType::ObjectColumn)) => kind,
        _ => {
            return Err(unsupported(
                "COMMENT ON an object other than a table or column",
                at,
            ));
        }
    };
    let names = match stmt.object.as_ref().and_then(|node| node.node.as_ref()) {
        Some(NodeEnum::List(list)) => types::idents(&list.items),
        _ => None,
    }
    .ok_or_else(|| unsupported("COMMENT ON a computed object name", at))?;
    let text = stmt.comment.clone();
    let named = |schema: Option<&String>, table: &String| TableIdent {
        table: table.clone(),
        schema: schema.cloned(),
    };
    match (kind, names.as_slice()) {
        (ObjectType::ObjectTable, [table]) => Ok(ParsedComment::Table {
            table: named(None, table),
            text,
        }),
        (ObjectType::ObjectTable, [schema, table]) => Ok(ParsedComment::Table {
            table: named(Some(schema), table),
            text,
        }),
        (ObjectType::ObjectColumn, [table, column]) => Ok(ParsedComment::Column {
            table: named(None, table),
            column: column.clone(),
            text,
        }),
        (ObjectType::ObjectColumn, [schema, table, column]) => Ok(ParsedComment::Column {
            table: named(Some(schema), table),
            column: column.clone(),
            text,
        }),
        _ => Err(unresolved("COMMENT ON names no readable object", at)),
    }
}
