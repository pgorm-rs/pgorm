"""Scalar expressions and predicates, and the statements built from them."""

from .emit_rust_models import GRAPH_SHAPES
from .emit_rust_values import (
    Q,
    REPLAY,
    UnsupportedInstruction,
    char_literal,
    literal,
    type_name,
    type_ref,
)

BIN_OPERATORS = {
    "eq": "Equal",
    "ne": "NotEqual",
    "gt": "GreaterThan",
    "gte": "GreaterThanOrEqual",
    "lt": "SmallerThan",
    "lte": "SmallerThanOrEqual",
    "add": "Add",
    "sub": "Sub",
    "mul": "Mul",
    "div": "Div",
    "mod": "Mod",
}
SCHEMA_TYPES = {
    "i16": "SmallInteger",
    "i32": "Integer",
    "i64": "BigInteger",
    "f32": "Float",
    "f64": "Double",
    "bool": "Boolean",
    "bytes": "Bytea",
    "decimal": "Decimal(None)",
    "json": "JsonBinary",
    "text": "Text",
    "uuid": "Uuid",
    "date": "Date",
    "time": "Time",
    "timestamp": "Timestamp",
    "timestamptz": "TimestampWithTimeZone",
}
JOIN_TYPES = {
    "inner": "InnerJoin",
    "left": "LeftJoin",
    "right": "RightJoin",
    "full": "FullOuterJoin",
}


# [spec:pgorm:req:generative.replay]
class ExprEmitter:
    """Emit an expression tree, and the statement or DDL built from one."""

    # -- expressions --------------------------------------------------------

    def expression(self, node, binder):
        name, d, i = node["op"], node["data"], node["inputs"]
        match name:
            case "value":
                return self.value_source(d["value"])
            case "name":
                self.helpers.add("identifier")
                return f"identifier(&{self.use(i['value'])})?"
            case "result.value":
                return self.result_value(node)
            case "table":
                return self.table_source(node)
            case "expr.column":
                return self.column_source(node)
            case "expr.value":
                return self.bound_value(i["value"], mode=d["mode"])
            case "expr.binary":
                return self.binary(node)
            case "expr.unary":
                value = self.use(i["value"])
                if d["operator"] == "not":
                    return f"{value}.not()"
                return f"{Q}::Expr::expr({value}).{d['operator']}()"
            case "expr.membership":
                method = "is_not_in" if d["negated"] else "is_in"
                items = ", ".join(self.coerce(item) for item in i["items"])
                value = f"{Q}::Expr::expr({self.use(i['value'])})"
                return f"{value}.{method}(vec![{items}])"
            case "expr.pattern":
                return self.pattern(node)
            case "expr.call":
                return self.function(node)
            case "expr.cast":
                kind = type_name(d["name"], d.get("schema"))
                if d.get("array"):
                    kind += ".array()"
                return f"{self.use(i['value'])}.cast_as_type({kind})"
            case "expr.alias":
                return f"({self.use(i['value'])}, {self.alias_of(node, 'name')})"
            case "expr.order":
                nulls = (
                    "None"
                    if d["nulls"] == "default"
                    else f"Some({Q}::NullOrdering::{d['nulls'].title()})"
                )
                order = f"{Q}::Order::{d['direction'].title()}"
                return f"({self.use(i['value'])}, {order}, {nulls})"
            case "condition":
                return self.condition(node)
            case "entity":
                return self.entity_path(node["id"])
            case "graph":
                factory, _ = GRAPH_SHAPES.get(d["name"], (None, None))
                if factory is None:
                    raise UnsupportedInstruction("no compiled graph for " + d["name"])
                return f"{REPLAY}::entities::graphs::{factory}"
            case "model":
                # A runtime model is a table plus a declared field map; only the
                # table is a value, and the map is read off the declaration at
                # each site that needs it.
                return self.use(i["table"])
            case "raw.template":
                items = ", ".join(self.use(item) for item in i["parameters"])
                return f"({literal(d['text'])}.to_owned(), {Q}::Values(vec![{items}]))"
            case _ if name.startswith("select") or name in (
                "insert",
                "update",
                "delete",
            ):
                return self.statement(node)
            case _ if name.startswith(("insert.", "write.", "update.")):
                return self.statement(node)
            case _ if name.startswith("schema."):
                return self.ddl(node)
            case _ if name.startswith(("entity.", "active.")):
                return self.compiled_entity(node)
            case _ if name.startswith(("graph.", "cursor.")):
                return self.compiled_graph(node)
            case _ if name.startswith("model."):
                return self.runtime_model(node)
            case _ if name.startswith("pipeline."):
                return self.pipeline(node, binder)
        raise UnsupportedInstruction("no public Rust source for " + name)

    def binary(self, node):
        d, i = node["data"], node["inputs"]
        left = self.coerce(i["left"])
        right = self.coerce(i["right"])
        if d["operator"] in ("and", "or"):
            return f"{left}.{d['operator']}({right})"
        operator = BIN_OPERATORS[d["operator"]]
        return f"{left}.binary({Q}::BinOper::{operator}, {right})"

    def pattern(self, node):
        d, i = node["data"], node["inputs"]
        value = self.use(i["value"])
        method = d["method"]
        if method in ("starts_with", "contains_text", "ends_with"):
            text = self.coerce(i["pattern"])
        if method == "starts_with":
            return f"{Q}::SimpleExpr::from({Q}::Func::starts_with({value}, {text}))"
        if method == "contains_text":
            call = (
                f'{Q}::Func::named({Q}::Alias::new("strpos")).args([{value}, {text}])'
            )
            zero = f"{Q}::SimpleExpr::Constant({Q}::Value::Int(Some(0i32)))"
            return f"{Q}::Expr::expr({call}).gt({zero})"
        if method == "ends_with":
            length = f"{Q}::SimpleExpr::from({Q}::Func::char_length({text}))"
            call = (
                f'{Q}::Func::named({Q}::Alias::new("right")).args([{value}, {length}])'
            )
            return f"{Q}::Expr::expr({call}).eq({text})"
        pattern = f"{Q}::LikeExpr::new({self.pattern_text(i['pattern'])})"
        escape = d.get("escape")
        if escape is not None:
            pattern += f".escape({char_literal(escape)})"
        if method in ("like", "not_like"):
            return f"{value}.{method}({pattern})"
        return f"{Q}::Expr::expr({value}).{method}({pattern})"

    def pattern_text(self, reference):
        node = self.nodes[reference]
        if node["op"] != "value":
            raise UnsupportedInstruction("a LIKE pattern needs a declared text value")
        snapshot = node["data"]["value"]
        if snapshot["sql_null"] or snapshot["type"]["kind"] != "text":
            raise UnsupportedInstruction("a LIKE pattern requires text")
        return literal(snapshot["data"]) + ".to_owned()"

    def function(self, node):
        d, i = node["data"], node["inputs"]
        arguments = [self.coerce(item) for item in i["arguments"]]
        name = d["name"]
        unary = (
            "lower",
            "upper",
            "abs",
            "char_length",
            "count",
            "count_distinct",
            "sum",
            "avg",
            "min",
            "max",
            "round",
        )
        if name in unary and len(arguments) == 1:
            call = f"{Q}::Func::{name}({arguments[0]})"
        elif name == "round" and len(arguments) == 2:
            call = f"{Q}::Func::round_with_precision({arguments[0]}, {arguments[1]})"
        elif name == "coalesce" and arguments:
            call = f"{Q}::Func::coalesce(vec![{', '.join(arguments)}])"
        elif name in ("random", "gen_random_uuid") and not arguments:
            call = f"{Q}::Func::{name}()"
        else:
            raise UnsupportedInstruction("unsupported function or arity: " + name)
        return f"{Q}::SimpleExpr::from({call})"

    def condition(self, node):
        d, i = node["data"], node["inputs"]
        result = f"{Q}::Condition::{d['mode']}()"
        for item in i["items"]:
            result += f".add({self.use(item)})"
        return result + ".not()" if d.get("negated") else result

    def predicate(self, reference):
        return self.use(reference)

    # -- statements ---------------------------------------------------------

    def projection(self, query, references):
        """Project expressions and aliased expressions the way `project` does."""
        lines = []
        for reference in references:
            if self.types[reference] == "projection":
                pair = self.use(reference)
                lines.append(f"    let (expression, name) = {pair};")
                lines.append(f"    {query}.expr_as(expression, name);")
            else:
                lines.append(f"    {query}.expr({self.use(reference)});")
        return lines

    def returning(self, references):
        if not references:
            return f"{Q}::Query::returning().all()"
        items = []
        for reference in references:
            if self.types[reference] == "projection":
                pair = self.use(reference)
                items.append(
                    f"{{ let (expression, name) = {pair}; "
                    f"expression.binary({Q}::BinOper::As, "
                    f"{Q}::SimpleExpr::from({Q}::Expr::col(name))) }}"
                )
            else:
                items.append(self.use(reference))
        return f"{Q}::Query::returning().exprs(vec![{', '.join(items)}])"

    def order_lines(self, keys):
        """Ordering over a mutable `query` binding, one declared key at a time."""
        lines = []
        for key in keys:
            lines.append(f"    let (expression, order, nulls) = {self.use(key)};")
            lines.append("    match nulls {")
            lines.append(
                "        Some(nulls) => "
                "query.order_by_expr_with_nulls(expression, order, nulls),"
            )
            lines.append("        None => query.order_by_expr(expression, order),")
            lines.append("    };")
        return lines

    def statement(self, node):
        name, d, i = node["op"], node["data"], node["inputs"]
        body = []
        match name:
            case "select":
                body.append(f"    let mut query = {Q}::Query::select();")
                if i["columns"]:
                    body.extend(self.projection("query", i["columns"]))
                else:
                    body.append(f"    query.column({Q}::Asterisk);")
            case "insert":
                table = self.use(i["table"])
                body.append(
                    f"    let mut query = {Q}::Query::insert()"
                    f".into_table({table}).to_owned();"
                )
                if d["columns"]:
                    items = ", ".join(
                        f"{Q}::Alias::new({literal(column)})" for column in d["columns"]
                    )
                    body.append(f"    query.columns(vec![{items}]);")
            case "update":
                table = self.use(i["table"])
                body.append(
                    f"    let mut query = {Q}::Query::update()"
                    f".table({table}).to_owned();"
                )
            case "delete":
                table = self.use(i["table"])
                body.append(
                    f"    let mut query = {Q}::Query::delete()"
                    f".from_table({table}).to_owned();"
                )
            case _:
                body.append(f"    let mut query = {self.use(i['query'])};")
                body.extend(self.mutation(node))
        if len(body) == 1 and body[0].startswith("    let mut query = n_"):
            # `write.all` only records intent the Rust builder does not carry.
            return self.use(i["query"])
        return "{\n" + "\n".join(body) + "\n    query\n    }"

    def mutation(self, node):
        name, d, i = node["op"], node["data"], node["inputs"]
        match name:
            case "select.from":
                return [f"    query.from({self.use(i['table'])});"]
            case "select.filter" | "write.filter":
                return [f"    query.cond_where({self.predicate(i['predicate'])});"]
            case "select.join":
                if d["kind"] == "cross":
                    return [f"    query.cross_join({self.use(i['table'])});"]
                if "on" not in i:
                    raise UnsupportedInstruction(
                        "select.join needs an on predicate for a " + d["kind"] + " join"
                    )
                join = f"{Q}::JoinType::{JOIN_TYPES[d['kind']]}"
                table = self.use(i["table"])
                return [f"    query.join({join}, {table}, {self.predicate(i['on'])});"]
            case "select.group":
                items = ", ".join(self.uses(i["keys"]))
                return [f"    query.add_group_by(vec![{items}]);"]
            case "select.having":
                return [f"    query.cond_having({self.predicate(i['predicate'])});"]
            case "select.order":
                return self.order_lines(i["keys"])
            case "select.page":
                lines = []
                for method in ("limit", "offset"):
                    if method in d:
                        lines.append(f"    query.{method}({int(d[method])}u64);")
                if not lines:
                    raise UnsupportedInstruction(
                        "select.page requires a limit or an offset"
                    )
                return lines
            case "select.distinct":
                return ["    query.distinct();"]
            case "insert.row":
                self.helpers.add("arity")
                items = ", ".join(self.coerce(value) for value in i["values"])
                return [
                    f"    arity(query.values(vec![{items}]).map(|_| ()))?;",
                ]
            case "insert.defaults":
                return ["    query.or_default_values();"]
            case "insert.conflict":
                return [f"    query.on_conflict({self.conflict(d)});"]
            case "update.set":
                column = f"{Q}::Alias::new({literal(d['column'])})"
                return [f"    query.value({column}, {self.coerce(i['value'])});"]
            case "write.all":
                return []
            case "write.returning":
                return [f"    query.returning({self.returning(i['columns'])});"]
        raise UnsupportedInstruction("no public Rust source for " + name)

    def conflict(self, data):
        keys = list(data["keys"])
        if not keys:
            raise UnsupportedInstruction("insert.conflict needs a target column")
        target = f"{Q}::OnConflict::column({Q}::Alias::new({literal(keys[0])}))"
        for key in keys[1:]:
            target += f".and_column({Q}::Alias::new({literal(key)}))"
        if data["action"] == "nothing":
            return target + ".do_nothing()"
        columns = data.get("columns")
        if not columns:
            raise UnsupportedInstruction(
                "insert.conflict update needs its assigned columns"
            )
        update = target
        for column in columns:
            update += f".update_column({Q}::Alias::new({literal(column)}))"
        return f"{Q}::OnConflict::from({update})"

    # -- DDL ----------------------------------------------------------------

    def data_type(self, kind):
        if isinstance(kind, dict):
            schema, name = kind["enum"]
            variants = "variants: Vec::new()"
            outer = (
                "None"
                if schema is None
                else f"Some({Q}::IntoName::into_name({Q}::Alias::new({literal(schema)})))"
            )
            local = f"{Q}::IntoName::into_name({Q}::Alias::new({literal(name)}))"
            return f"{Q}::ColumnType::Enum {{ name: {local}, schema: {outer}, {variants} }}"
        array = kind.endswith("[]")
        base = kind[:-2] if array else kind
        if base not in SCHEMA_TYPES:
            raise UnsupportedInstruction("schema column kind has no Rust type: " + base)
        result = f"{Q}::ColumnType::{SCHEMA_TYPES[base]}"
        if array:
            return f"{Q}::ColumnType::Array(std::sync::Arc::new({result}))"
        return result

    def ddl(self, node):
        name, d, i = node["op"], node["data"], node["inputs"]
        match name:
            case "schema.create":
                table = self.ddl_table(i["table"])
                body = [f"    let mut statement = {Q}::Table::create({table});"]
                for column in d["columns"]:
                    definition = (
                        f"{Q}::ColumnDef::new_with_type("
                        f"{Q}::Alias::new({literal(column['name'])}), "
                        f"{self.data_type(column['kind'])})"
                    )
                    body.append(f"    let mut column = {definition};")
                    body.append(
                        "    column."
                        + ("null();" if column["nullable"] else "not_null();")
                    )
                    if column["primary"]:
                        body.append("    column.primary_key();")
                    body.append("    statement.col(column);")
                body.append("    statement.to_string()")
                return "{\n" + "\n".join(body) + "\n    }"
            case "schema.drop":
                return f"{Q}::Table::drop({self.ddl_table(i['table'])}).to_string()"
            case "schema.rename":
                table = self.ddl_table(i["table"])
                target = f"{Q}::Alias::new({literal(d['name'])})"
                if "column" in d:
                    column = f"{Q}::Alias::new({literal(d['column'])})"
                    return (
                        f"{Q}::Table::rename_column({table}, {column}, {target})"
                        ".to_string()"
                    )
                return f"{Q}::Table::rename({table}, {target}).to_string()"
            case "schema.index":
                if not d["columns"]:
                    raise UnsupportedInstruction("schema.index needs a first column")
                first, *rest = d["columns"]
                table = self.ddl_table(i["table"])
                body = [
                    f"    let mut statement = {Q}::Index::create("
                    f"{table}, {Q}::Alias::new({literal(first)}));",
                    f"    statement.name({Q}::Alias::new({literal(d['name'])}));",
                ]
                for column in rest:
                    body.append(
                        f"    statement.col({Q}::Alias::new({literal(column)}));"
                    )
                if d["unique"]:
                    body.append("    statement.unique();")
                body.append("    statement.to_string()")
                return "{\n" + "\n".join(body) + "\n    }"
            case "schema.enum":
                labels = ", ".join(
                    f"{Q}::Alias::new({literal(label)})" for label in d["labels"]
                )
                body = [
                    f"    let mut statement = {Q}::extension::Type::create("
                    f"{type_ref(d['name'], d['schema'])});",
                    f"    statement.as_enum().values(vec![{labels}]);",
                    "    statement.to_string()",
                ]
                return "{\n" + "\n".join(body) + "\n    }"
            case "schema.enum_change":
                return self.enum_change(d)
        raise UnsupportedInstruction("no public Rust source for " + name)

    def ddl_table(self, reference):
        schema, name, alias = self.table_identity(reference)
        if alias is not None:
            raise UnsupportedInstruction("DDL table targets cannot have an alias")
        inner = f"{Q}::IntoName::into_name({Q}::Alias::new({literal(name)}))"
        if schema is None:
            return f"{Q}::TableName::Table({inner})"
        outer = f"{Q}::IntoName::into_name({Q}::Alias::new({literal(schema)}))"
        return f"{Q}::TableName::SchemaTable({outer}, {inner})"

    def enum_change(self, data):
        target = type_ref(data["name"], data["schema"])
        if data["method"] == "drop":
            return f"{Q}::extension::Type::drop({target}).to_string()"
        if "value" not in data:
            raise UnsupportedInstruction(
                "schema.enum_change " + data["method"] + " needs a value"
            )
        label = f"{Q}::Alias::new({literal(data['value'])})"
        if data["method"] == "add":
            return (
                f"{Q}::extension::Type::alter({target}).add_value({label}).to_string()"
            )
        if "new_value" not in data:
            raise UnsupportedInstruction(
                "schema.enum_change rename needs its replacement value"
            )
        renamed = f"{Q}::Alias::new({literal(data['new_value'])})"
        return (
            f"{Q}::extension::Type::alter({target})"
            f".rename_value({label}, {renamed}).to_string()"
        )
