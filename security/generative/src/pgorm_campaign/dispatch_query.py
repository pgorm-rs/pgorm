"""Native SELECT and guarded CRUD builder dispatch."""


def dispatch(name, i, d, p):
    match name:
        case "select":
            return p.Select(*i["columns"]), [
                "pgorm_query::Query::select",
                "pgorm_query::SelectStatement::exprs",
            ]
        case "select.from":
            return i["query"].from_(i["table"]), ["pgorm_query::SelectStatement::from"]
        case "select.filter" | "write.filter":
            return i["query"].where_(i["predicate"]), [
                "pgorm_query::ConditionalStatement::cond_where"
            ]
        case "select.join":
            if d["kind"] == "cross":
                return i["query"].cross_join(i["table"]), [
                    "pgorm_query::SelectStatement::cross_join"
                ]
            return i["query"].join(
                i["table"], i["on"], kind=getattr(p.Join, d["kind"].title())
            ), ["pgorm_query::SelectStatement::join"]
        case "select.group":
            return i["query"].group_by(*i["keys"]), [
                "pgorm_query::SelectStatement::group_by_columns"
            ]
        case "select.having":
            return i["query"].having(i["predicate"]), [
                "pgorm_query::SelectStatement::cond_having"
            ]
        case "select.order":
            return i["query"].order_by(*i["keys"]), ["pgorm_query::OrderedStatement"]
        case "select.page" | "entity.page":
            query, paths = i["query"], []
            prefix = (
                "pgorm_query::SelectStatement"
                if name == "select.page"
                else "pgorm::QuerySelect"
            )
            for method in ("limit", "offset"):
                if method in d:
                    query = getattr(query, method)(d[method])
                    paths.append(prefix + "::" + method)
            if not paths:
                raise p.ConstructionError("pagination requires a limit or offset")
            return query, paths
        case "select.distinct":
            return i["query"].distinct(), ["pgorm_query::SelectStatement::distinct"]
        case "insert":
            query = p.Insert(i["table"])
            paths = [
                "pgorm_query::Query::insert",
                "pgorm_query::InsertStatement::into_table",
            ]
            if d["columns"]:
                query = query.columns(*d["columns"])
                paths.append("pgorm_query::InsertStatement::columns")
            return query, paths
        case "insert.row":
            return i["query"].values(*i["values"]), [
                "pgorm_query::InsertStatement::values"
            ]
        case "insert.defaults":
            return i["query"].default_values(), [
                "pgorm_query::InsertStatement::or_default_values"
            ]
        case "insert.conflict":
            target = p.ConflictTarget(*d["keys"])
            action = (
                target.ignore()
                if d["action"] == "nothing"
                else target.update(*d["columns"])
            )
            return i["query"].on_conflict(action), ["pgorm_query::OnConflict"]
        case "update":
            return p.Update(i["table"]), ["pgorm_query::Query::update"]
        case "update.set":
            return i["query"].set(d["column"], i["value"]), [
                "pgorm_query::UpdateStatement::value"
            ]
        case "delete":
            return p.Delete(i["table"]), ["pgorm_query::Query::delete"]
        case "write.all":
            path = (
                "UpdateStatement"
                if isinstance(i["query"], p.Update)
                else "DeleteStatement"
            )
            return i["query"].all_rows(), ["pgorm_query::" + path]
        case "write.returning":
            return i["query"].returning(*i["columns"]), ["pgorm_query::ReturningClause"]
        case "raw.template":
            query = p.RawSQL(d["text"], i["parameters"])
            query.inline_sql()
            return query, ["pgorm_query::inject_parameters"]
    raise RuntimeError("inactive query dispatch: " + name)
