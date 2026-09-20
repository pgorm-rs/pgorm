"""Pipeline relations and stages, each callback owning its binder scope."""

from . import wire
from .emit_rust_values import PL, Q, UnsupportedInstruction, boolean, literal
from .program import CALLBACK_INPUTS

PIPELINE_BINARY = {
    "eq": "eq",
    "ne": "ne",
    "gt": "gt",
    "gte": "gte",
    "lt": "lt",
    "lte": "lte",
    "and": "and",
    "or": "or",
    "add": "add",
    "sub": "sub",
    "mul": "mul",
    "div": "div",
    "mod": "rem",
}
PIPELINE_CASTS = {
    "smallint": "SmallInt",
    "integer": "Integer",
    "bigint": "BigInt",
    "real": "Real",
    "double": "Double",
    "float8": "Double",
    "numeric": "Numeric",
    "text": "Text",
    "boolean": "Boolean",
    "date": "Date",
    "timestamp": "Timestamp",
    "timestamptz": "Timestamptz",
    "interval": "Interval",
    "uuid": "Uuid",
    "json": "Json",
    "jsonb": "Jsonb",
}
JOIN_SIDES = {"inner": "Inner", "left": "Left", "right": "Right", "full": "Full"}


# [spec:pgorm:req:generative.replay]
class PipelineEmitter:
    """Emit a pipeline relation and the transforms applied to it."""

    def relation(self, reference):
        """A pipeline relation descriptor: (schema, name, alias) or a pipeline."""
        kind = self.types[reference]
        if kind == "pipeline":
            return ("pipeline", self.use(reference), None)
        if kind == "entity":
            self.entity_path(reference)
            name = self.nodes[reference]["data"]["name"]
            table = "accounts" if name == "campaign.Account" else "notes"
            return ("table", "fixture", table)
        if kind == "table":
            schema, name, alias = self.table_identity(reference)
            return ("table", schema, name)
        if kind == "source":
            node = self.nodes[reference]
            return self.relation(node["inputs"]["source"])
        raise UnsupportedInstruction("no pipeline relation for " + kind)

    def source_alias(self, reference):
        node = self.nodes[reference]
        if self.types[reference] == "source":
            if "alias" in node["data"]:
                return node["data"]["alias"]
            return self.source_alias(node["inputs"]["source"])
        if self.types[reference] == "table":
            return self.table_identity(reference)[2]
        return None

    def pipeline_from(self, reference):
        """`PySource::pipeline`: a bare qualified table opens with from_schema."""
        kind, first, second = self.relation(reference)
        alias = self.source_alias(reference)
        if kind == "table" and alias is None:
            if first is None:
                return f"{PL}::Pipeline::from({Q}::Name::runtime({literal(second)}))"
            return (
                f"{PL}::Pipeline::from_schema({Q}::Name::runtime({literal(first)}), "
                f"{Q}::Name::runtime({literal(second)}))"
            )
        return f"{PL}::Pipeline::from({self.pipeline_operand(reference)})"

    def pipeline_operand(self, reference):
        """`PySource::source`: the relation as a join, append or `from` operand."""
        kind, first, second = self.relation(reference)
        alias = self.source_alias(reference)
        if kind == "pipeline":
            source = f"{PL}::IntoSource::into_source({first})"
        elif first is None:
            source = (
                f"{PL}::IntoSource::into_source({Q}::Name::runtime({literal(second)}))"
            )
        else:
            inner = (
                f"{PL}::Pipeline::from_schema({Q}::Name::runtime({literal(first)}), "
                f"{Q}::Name::runtime({literal(second)}))"
            )
            name = f"{Q}::Name::runtime({literal(alias if alias else second)})"
            source = (
                f"{PL}::IntoSource::into_source({PL}::named_runtime({inner}, {name}))"
            )
        if alias is not None:
            named = (
                f"{PL}::named_runtime({source}, {Q}::Name::runtime({literal(alias)}))"
            )
            return f"{PL}::IntoSource::into_source({named})"
        return source

    def pipeline_list(self, references):
        if not references:
            raise UnsupportedInstruction("a pipeline projection needs a column")
        return "[" + ", ".join(self.uses(references)) + "]"

    def pipeline_value(self, node):
        source = self.nodes[node["inputs"]["value"]]
        if source["op"] != "value":
            raise UnsupportedInstruction("pipeline.value needs a declared literal")
        snapshot = source["data"]["value"]
        wire.validate(snapshot)
        tag = snapshot["type"]
        if snapshot["sql_null"]:
            return f"{PL}::null()"
        if tag["kind"] == "bool":
            return f"{PL}::Expr::from({boolean(snapshot['data'])})"
        if tag["kind"] in ("i32", "i64"):
            return f"{PL}::Expr::from({int(snapshot['data'])}i64)"
        if tag["kind"] == "f64":
            bits = snapshot["data"]
            return f"{PL}::Expr::from(f64::from_bits(0x{bits}u64))"
        if tag["kind"] == "text":
            return f"{PL}::Expr::from({literal(snapshot['data'])})"
        raise UnsupportedInstruction(
            "pipeline.value has no Rust literal for " + tag["kind"]
        )

    # -- pipeline stages ----------------------------------------------------

    def pipeline(self, node, binder):
        name, d, i = node["op"], node["data"], node["inputs"]
        match name:
            case "pipeline.source":
                # The relation stays a descriptor; how it lowers depends on the
                # stage that consumes it, exactly as `PySource` defers.
                return self.pipeline_operand(node["id"])
            case "pipeline.from":
                return self.pipeline_from(i["source"])
            case "pipeline.column":
                source = f"{Q}::Name::runtime({literal(d['source'])})"
                column = f"{Q}::Name::runtime({literal(d['column'])})"
                return f"{PL}::col({source}, {column})"
            case "pipeline.alias":
                return f"{PL}::Expr::from({Q}::Name::runtime({literal(d['name'])}))"
            case "pipeline.value":
                return self.pipeline_value(node)
            case "pipeline.bind":
                if binder is None:
                    raise UnsupportedInstruction(
                        "pipeline.bind has no owning public callback"
                    )
                return f"{binder}.bind({self.use(i['value'])})"
            case "pipeline.binary":
                method = PIPELINE_BINARY[d["operator"]]
                left, right = self.use(i["left"]), self.use(i["right"])
                return f"{PL}::ExprOps::{method}({left}, {right})"
            case "pipeline.unary":
                value = self.use(i["value"])
                if d["operator"] == "not":
                    return f"!({value})"
                if d["operator"] == "neg":
                    return f"-({value})"
                return f"{PL}::ExprOps::{d['operator']}({value})"
            case "pipeline.membership":
                items = ", ".join(self.uses(i["items"]))
                return f"{PL}::ExprOps::in_array({self.use(i['value'])}, [{items}])"
            case "pipeline.function":
                arguments = ", ".join(self.uses(i["arguments"]))
                return f"{PL}::{d['name']}({arguments})"
            case "pipeline.cast":
                kind = PIPELINE_CASTS.get(d["name"])
                if kind is None:
                    raise UnsupportedInstruction(
                        "pipeline.cast has no CastType for " + d["name"]
                    )
                value = self.use(i["value"])
                return f"{PL}::ExprOps::cast({value}, {PL}::CastType::{kind})"
            case "pipeline.named":
                value = self.use(i["value"])
                name_source = f"{Q}::Name::runtime({literal(d['name'])})"
                return f"{PL}::ExprOps::as_runtime({value}, {name_source})"
            case "pipeline.take":
                query = self.use(i["query"])
                return f"{query}.take_range({int(d['start'])}i64..={int(d['end'])}i64)"
            case "pipeline.set":
                operand = self.pipeline_operand(i["source"])
                return f"{self.use(i['query'])}.{d['method']}({operand})"
            case "pipeline.distinct":
                return f"{self.use(i['query'])}.distinct()"
            case "pipeline.sources":
                _, qualifiers, entities = self.source_shape(node["id"])
                listed = [
                    f"{PL}::named_runtime({self.entity_type(entity)}::default(), "
                    f"{Q}::Name::runtime({literal(qualifier)}))"
                    for entity, qualifier in zip(entities, qualifiers, strict=True)
                ]
                # One listed source is the source itself, not a one-tuple: that
                # is the shape `SourceList` is implemented for, and the row it
                # decodes is a bare `Option<Model>`.
                sources = (
                    listed[0] if len(listed) == 1 else "(" + ", ".join(listed) + ",)"
                )
                query = self.use(i["query"])
                return f"{PL}::Pipeline::select_sources({query}, {sources})"
        raise UnsupportedInstruction("no public Rust source for " + name)

    def stage(self, node):
        """A pipeline transform, with the callback owning its binder scope."""
        name, d, i = node["op"], node["data"], node["inputs"]
        method = name.removeprefix("pipeline.")
        references = i[next(iter(CALLBACK_INPUTS[name]))]
        query = self.use(i["query"])
        bound = "binder" in d
        body = []
        if bound:
            for identity in self.owned[d["binder"]]:
                inner = self.nodes[identity]
                body.append(
                    f"        let {self.var(identity)} = "
                    f"{self.expression(inner, 'binder')};"
                )
            returned = (
                self.pipeline_list(references)
                if isinstance(references, list)
                else self.use(references)
            )
            body.append("        " + returned)
            closure = "|binder| {\n" + "\n".join(body) + "\n    }"
        else:
            closure = None
        if method == "window":
            # An empty key list has no `ExprList` element type to infer, so an
            # absent partition or ordering is left off the spec entirely.
            over = f"{PL}::over()"
            if i["partition"]:
                over += f".by({self.pipeline_list(i['partition'])})"
            if i["order"]:
                over += f".sort_by({self.pipeline_list(i['order'])})"
            if "start" in d or "end" in d:
                start = "None" if "start" not in d else f"Some({int(d['start'])}i64)"
                end = "None" if "end" not in d else f"Some({int(d['end'])}i64)"
                over += f".rows({start}, {end})"
            call = (
                f"{query}.window_with({over}, {closure})"
                if bound
                else f"{query}.window({self.pipeline_list(references)}, {over})"
            )
        elif method == "join":
            side = f"{PL}::JoinSide::{JOIN_SIDES[d['kind']]}"
            operand = self.pipeline_operand(i["source"])
            call = (
                f"{query}.join_with({side}, {operand}, {closure})"
                if bound
                else f"{query}.join({side}, {operand}, {self.use(references)})"
            )
        elif bound:
            call = f"{query}.{method}_with({closure})"
        elif isinstance(references, list):
            call = f"{query}.{method}({self.pipeline_list(references)})"
        else:
            call = f"{query}.{method}({self.use(references)})"
        return [f"    let {self.var(node['id'])} = {call};"]
