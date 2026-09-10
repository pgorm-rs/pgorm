"""DDL uses the public schema builder facade, independent of fixture setup SQL."""

TYPES = {
    "i16": "smallint",
    "i32": "integer",
    "i64": "bigint",
    "f32": "real",
    "f64": "double",
    "bool": "boolean",
    "bytes": "bytea",
    "decimal": "numeric",
    "json": "jsonb",
}


def datatype(kind, p, schema):
    if isinstance(kind, dict):
        namespace, name = kind["enum"]
        return schema.DataType(p.TypeName(name, schema=namespace))
    array = kind.endswith("[]")
    base = kind[:-2] if array else kind
    result = schema.DataType(TYPES.get(base, base))
    return result.array() if array else result


def dispatch(name, i, d, p):
    from pgorm import schema

    match name:
        case "schema.create":
            query = schema.CreateTable(i["table"])
            for column in d["columns"]:
                value = schema.ColumnDef(
                    column["name"], datatype(column["kind"], p, schema)
                )
                value = value.null() if column["nullable"] else value.not_null()
                if column["primary"]:
                    value = value.primary_key()
                query = query.column(value)
            return query, ["pgorm_query::Table::create", "pgorm_query::ColumnDef"]
        case "schema.drop":
            return schema.drop_table(i["table"]), ["pgorm_query::Table::drop"]
        case "schema.rename":
            if "column" in d:
                return schema.rename_column(i["table"], d["column"], d["name"]), [
                    "pgorm_query::TableAlterStatement::rename_column"
                ]
            return schema.rename_table(i["table"], d["name"]), [
                "pgorm_query::Table::rename"
            ]
        case "schema.index":
            first, *rest = d["columns"]
            query = schema.CreateIndex(i["table"], first, name=d["name"])
            for column in rest:
                query = query.column(column)
            return (query.unique() if d["unique"] else query), [
                "pgorm_query::Index::create"
            ]
        case "schema.enum":
            return schema.create_enum(
                p.TypeName(d["name"], schema=d["schema"]), d["labels"]
            ), ["pgorm_query::extension::postgres::Type::create"]
        case "schema.enum_change":
            kind = p.TypeName(d["name"], schema=d["schema"])
            match d["method"]:
                case "add":
                    return schema.add_enum_value(kind, d["value"]), [
                        "pgorm_query::extension::postgres::Type::alter"
                    ]
                case "rename":
                    return schema.rename_enum_value(kind, d["value"], d["new_value"]), [
                        "pgorm_query::extension::postgres::Type::alter"
                    ]
                case "drop":
                    return schema.drop_enum(kind), [
                        "pgorm_query::extension::postgres::Type::drop"
                    ]
    raise RuntimeError("inactive schema dispatch: " + name)
