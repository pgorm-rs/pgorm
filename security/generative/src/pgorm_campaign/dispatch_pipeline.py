"""Pipeline stages retain real native callback binder ownership."""

from .dispatch_expr import binary, unary
from .program import CALLBACK_INPUTS


def stage(node, resolve, p):
    from pgorm import pipeline as pl

    name, inputs, d = node["op"], node["inputs"], node["data"]
    method = name.removeprefix("pipeline.")
    query = resolve(inputs["query"])
    key = next(iter(CALLBACK_INPUTS[name]))
    references = inputs[key]
    bound = "binder" in d

    def evaluate(binder=None):
        cache = {}

        def get(reference):
            return resolve(
                reference, scope=d.get("binder", "root"), binder=binder, local=cache
            )

        return (
            [get(ref) for ref in references]
            if isinstance(references, list)
            else get(references)
        )

    arguments = [evaluate] if bound else evaluate()
    if not isinstance(arguments, list):
        arguments = [arguments]
    suffix = "_with" if bound else ""
    paths = [
        "pgorm::pipeline::"
        + ("Grouped" if method == "aggregate" else "Pipeline")
        + "::"
        + method
        + suffix
    ]
    if method == "window":
        over = pl.Over().by(*(resolve(ref) for ref in inputs["partition"]))
        over = over.sort_by(*(resolve(ref) for ref in inputs["order"]))
        if "start" in d or "end" in d:
            over = over.rows(d.get("start"), d.get("end"))
        paths.append("pgorm::pipeline::Over")
        result = (
            query.window_with(over, evaluate)
            if bound
            else query.window(*arguments, over=over)
        )
    elif method == "join":
        source = resolve(inputs["source"])
        result = getattr(query, "join" + suffix)(
            source, *arguments, kind=getattr(p.Join, d["kind"].title())
        )
    else:
        result = getattr(query, method + suffix)(*arguments)
    return result, paths


def dispatch(name, i, d, p, binder):
    from pgorm import pipeline as pl

    match name:
        case "pipeline.source":
            source = pl.source(i["source"])
            paths = ["pgorm::pipeline::IntoSource"]
            if "alias" in d:
                source = source.named(d["alias"])
                paths.append("pgorm::pipeline::Source::named")
            return source, paths
        case "pipeline.from":
            return pl.Pipeline(i["source"]), ["pgorm::pipeline::Pipeline::from"]
        case "pipeline.column":
            return pl.col(d["source"], d["column"]), ["pgorm::pipeline::col"]
        case "pipeline.alias":
            return pl.alias(d["name"]), ["pgorm::pipeline::alias"]
        case "pipeline.value":
            return pl.literal(i["value"]), ["pgorm::pipeline::literal"]
        case "pipeline.bind":
            if binder is None:
                raise RuntimeError("binder instruction has no active public callback")
            return binder.bind(i["value"]), ["pgorm::pipeline::Binder::bind"]
        case "pipeline.binary":
            return binary(i["left"], i["right"], d["operator"]), [
                "pgorm::pipeline::ExprOps"
            ]
        case "pipeline.unary":
            return unary(i["value"], d["operator"]), ["pgorm::pipeline::ExprOps"]
        case "pipeline.membership":
            return i["value"].in_array(i["items"]), [
                "pgorm::pipeline::ExprOps::in_array"
            ]
        case "pipeline.function":
            return getattr(pl, d["name"])(*i["arguments"]), [
                "pgorm::pipeline::" + d["name"]
            ]
        case "pipeline.cast":
            return i["value"].cast(d["name"]), ["pgorm::pipeline::ExprOps::cast"]
        case "pipeline.named":
            return i["value"].as_(d["name"]), ["pgorm::pipeline::ExprOps::as_"]
        case "pipeline.take":
            return i["query"].take_range(d["start"], d["end"]), [
                "pgorm::pipeline::Pipeline::take_range"
            ]
        case "pipeline.set":
            return getattr(i["query"], d["method"])(i["source"]), [
                "pgorm::pipeline::Pipeline::" + d["method"]
            ]
        case "pipeline.distinct":
            return i["query"].distinct(), ["pgorm::pipeline::Pipeline::distinct"]
        case "pipeline.sources":
            selection = pl.sources(d["name"])
            return i["query"].select_sources(selection, qualifiers=d["qualifiers"]), [
                "pgorm::pipeline::Pipeline::select_sources"
            ]
    raise RuntimeError("inactive pipeline dispatch: " + name)
