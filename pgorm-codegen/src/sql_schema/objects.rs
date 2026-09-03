use super::{types, unresolved, unsupported};
use crate::Error;
use pg_query::NodeEnum;
use pg_query::protobuf::{
    CommentStmt, CreateEnumStmt, IndexStmt, ObjectType, SortByDir, SortByNulls,
};
use pgorm_query::{
    Alias, Index, IndexCreateStatement, IndexOrder, IndexType, IntoIndexColumn as _,
};

/// A `CREATE TYPE ... AS ENUM` as the name and values a column of that type
/// carries into `ColumnType::Enum`.
// [spec:pgorm:sem:codegen.ddl.objects]
pub(super) fn enum_type(stmt: &CreateEnumStmt, at: usize) -> Result<(String, Vec<String>), Error> {
    let names = types::idents(&stmt.type_name)
        .ok_or_else(|| unsupported("a computed type name in CREATE TYPE", at))?;
    let Some(name) = names.last() else {
        return Err(unresolved("CREATE TYPE without a type name", at));
    };
    let values = types::idents(&stmt.vals)
        .ok_or_else(|| unsupported("a computed value in CREATE TYPE", at))?;
    Ok((name.clone(), values))
}

/// A `CREATE INDEX`, and the table it belongs to.
pub(super) struct ParsedIndex {
    pub(super) table: String,
    /// The index as the table statement carries it — `None` for an index that
    /// states no entity fact, which has no place inside a `CREATE TABLE`.
    pub(super) index: Option<IndexCreateStatement>,
}

// [spec:pgorm:sem:codegen.ddl.objects]
pub(super) fn index(stmt: &IndexStmt, at: usize) -> Result<ParsedIndex, Error> {
    let table = match stmt.relation.as_ref() {
        Some(relation) if !relation.relname.is_empty() => relation.relname.clone(),
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
        let column = Alias::new(element.name.as_str());
        columns.push(match SortByDir::try_from(element.ordering) {
            Ok(SortByDir::SortbyDefault) => column.into_index_column(),
            Ok(SortByDir::SortbyAsc) => (column, IndexOrder::Asc).into_index_column(),
            Ok(SortByDir::SortbyDesc) => (column, IndexOrder::Desc).into_index_column(),
            _ => return Err(on("an index column ordering")),
        });
    }

    let mut columns = columns.into_iter();
    let Some(first) = columns.next() else {
        return Err(on("an index over no columns"));
    };
    let mut index = Index::create(Alias::new(table.as_str()), first);
    if !stmt.idxname.is_empty() {
        index.name(stmt.idxname.as_str());
    }
    if stmt.if_not_exists {
        index.if_not_exists();
    }
    if stmt.nulls_not_distinct {
        index.nulls_not_distinct();
    }
    if stmt.unique {
        index.unique();
    }
    if let Some(index_type) = index_type(&stmt.access_method) {
        index.index_type(index_type);
    }
    for column in columns {
        index.col(column);
    }
    Ok(ParsedIndex {
        table,
        index: stmt.unique.then(|| index.take()),
    })
}

fn index_type(access_method: &str) -> Option<IndexType> {
    match access_method {
        "" | "btree" => None,
        "hash" => Some(IndexType::Hash),
        "gin" => Some(IndexType::FullText),
        other => Some(IndexType::Custom(pgorm_query::SeaRc::new(Alias::new(
            other,
        )))),
    }
}

/// A `COMMENT ON` the bridge can attach to a table or one of its columns.
pub(super) enum ParsedComment {
    Table {
        table: String,
        text: String,
    },
    Column {
        table: String,
        column: String,
        text: String,
    },
}

impl ParsedComment {
    pub(super) fn table(&self) -> &str {
        match self {
            Self::Table { table, .. } | Self::Column { table, .. } => table,
        }
    }
}

// [spec:pgorm:sem:codegen.ddl.objects]
pub(super) fn comment(stmt: &CommentStmt, at: usize) -> Result<ParsedComment, Error> {
    let target = match ObjectType::try_from(stmt.objtype) {
        Ok(target @ (ObjectType::ObjectTable | ObjectType::ObjectColumn)) => target,
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
    match (target, names.as_slice()) {
        (ObjectType::ObjectTable, [table] | [_, table]) => Ok(ParsedComment::Table {
            table: table.clone(),
            text,
        }),
        (ObjectType::ObjectColumn, [table, column] | [_, table, column]) => {
            Ok(ParsedComment::Column {
                table: table.clone(),
                column: column.clone(),
                text,
            })
        }
        _ => Err(unresolved("COMMENT ON names no readable object", at)),
    }
}
