"""Versioned operation graphs, ordered effects and explicit observation contracts."""

from dataclasses import dataclass
import hashlib
import json

from . import baseline, catalog, parameters, wire

VERSION = 1
MAX_BYTES = 2**20
MAX_NODES = 256
MAX_STEPS = 128
CALLBACK_INPUTS = {
    "pipeline.filter": {"predicate"},
    "pipeline.derive": {"columns"},
    "pipeline.select": {"columns"},
    "pipeline.group": {"keys"},
    "pipeline.aggregate": {"columns"},
    "pipeline.window": {"columns"},
    "pipeline.sort": {"keys"},
    "pipeline.join": {"on"},
}


def _pairs(items):
    result = {}
    for key, value in items:
        if key in result:
            raise wire.FormatError("duplicate JSON object key: " + key)
        result[key] = value
    return result


def _nonfinite(value):
    raise wire.FormatError("nonstandard JSON number: " + value)


def _budget(value, depth=0):
    if depth > 64:
        raise wire.FormatError("program exceeds its nesting budget")
    if isinstance(value, dict):
        for item in value.values():
            _budget(item, depth + 1)
    elif isinstance(value, list):
        if len(value) > 4096:
            raise wire.FormatError("program list exceeds its item budget")
        for item in value:
            _budget(item, depth + 1)
    elif type(value) not in (str, int, float, bool, type(None)):
        raise wire.FormatError("program contains a non-JSON value")


def _list(value, limit, *, empty=False):
    if not isinstance(value, list) or not int(not empty) <= len(value) <= limit:
        raise wire.FormatError(
            "program inventory is empty or exceeds its declared limit"
        )
    return value


def _binders(value):
    binders = {}
    for item in _list(value, 64, empty=True):
        wire.fields(item, {"id", "owner"})
        identity = parameters.identity(item["id"])
        parameters.identity(item["owner"])
        if identity == "root" or identity in binders:
            raise wire.FormatError("duplicate or reserved binder identity")
        binders[identity] = item["owner"]
    return binders


def _scope(node, nodes, binders):
    scope = node["scope"]
    if scope != "root" and scope not in binders:
        raise wire.FormatError("unknown binder scope")
    if node["op"] == "pipeline.bind" and scope == "root":
        raise wire.FormatError("bound pipeline values require an owning binder")
    binder = node["data"].get("binder")
    if binder is not None and (scope != "root" or binders.get(binder) != node["id"]):
        raise wire.FormatError("callback does not own its declared binder scope")
    for key, value in node["inputs"].items():
        for reference in value if isinstance(value, list) else [value]:
            source_scope = nodes[reference]["scope"]
            if source_scope in ("root", scope):
                continue
            if source_scope == binder and key in CALLBACK_INPUTS.get(node["op"], set()):
                continue
            raise wire.FormatError("a value escaped its owning binder scope")


def _nodes(program, binders):
    nodes, types, depths, results = {}, {}, {}, {}
    for node in _list(program["nodes"], MAX_NODES):
        wire.fields(node, {"id", "op", "scope", "inputs", "data"})
        identity = parameters.identity(node["id"])
        if identity in nodes or identity == "root":
            raise wire.FormatError("duplicate or reserved instruction identity")
        operation = catalog.OPERATIONS.get(node["op"])
        if operation is None:
            raise wire.FormatError("unknown operation: " + str(node["op"]))
        parameters.options(node["data"], operation.data)
        parameters.inputs(node["inputs"], operation.inputs, types)
        if node["op"] in ("table", "expr.column", "expr.alias"):
            if ("name" in node["data"]) == ("name" in node["inputs"]):
                raise wire.FormatError(
                    "a name requires exactly one data or identifier-node input"
                )
        _scope(node, nodes, binders)
        references = list(parameters.input_ids(node["inputs"]))
        depths[identity] = 1 + max((depths[ref] for ref in references), default=0)
        if depths[identity] > 64:
            raise wire.FormatError(
                "operation graph exceeds its dependency-depth budget"
            )
        results[identity] = set().union(*(results[ref] for ref in references))
        if node["op"] == "result.value":
            results[identity].add(node["data"]["step"])
        output = operation.output
        types[identity] = types[node["inputs"]["query"]] if output == "same" else output
        nodes[identity] = node
    for binder, owner in binders.items():
        if owner not in nodes or nodes[owner]["data"].get("binder") != binder:
            raise wire.FormatError("binder has no consuming callback")
    return nodes, types, results


def _steps(program, nodes, types, results):
    steps = {}
    stack = ["root"]
    scopes = {"root"}
    for step in _list(program["steps"], MAX_STEPS):
        wire.fields(step, {"id", "op", "scope", "inputs", "data"})
        identity = parameters.identity(step["id"])
        if identity in steps or identity in nodes or identity in ("root", "final"):
            raise wire.FormatError("duplicate or reserved effect identity")
        operation = catalog.EFFECTS.get(step["op"])
        if operation is None:
            raise wire.FormatError("unknown effect")
        parameters.options(step["data"], operation.data)
        parameters.inputs(step["inputs"], operation.inputs, types)
        if step["scope"] != stack[-1]:
            raise wire.FormatError(
                "effect uses an inactive or reserved transaction parent"
            )
        for reference in parameters.input_ids(step["inputs"]):
            if nodes[reference]["scope"] != "root":
                raise wire.FormatError("effect consumes an escaped binder value")
            if not results[reference] <= steps.keys():
                raise wire.FormatError(
                    "effect depends on an unavailable earlier result"
                )
            for result in results[reference]:
                if catalog.EFFECTS[steps[result]["op"]].output not in ("rows", "model"):
                    raise wire.FormatError(
                        "result field reference does not name a row result"
                    )
        if step["op"] == "begin":
            child = step["data"]["child"]
            if child in scopes or len(stack) >= 8:
                raise wire.FormatError(
                    "duplicate transaction scope or excessive nesting"
                )
            scopes.add(child)
            stack.append(child)
        elif step["op"] in ("commit", "rollback"):
            if len(stack) == 1:
                raise wire.FormatError("root connection is not a transaction")
            stack.pop()
        elif step["op"] == "stream" and len(stack) != 1:
            raise wire.FormatError(
                "the public binding does not expose transaction streams"
            )
        steps[identity] = step
    if len(stack) != 1:
        raise wire.FormatError("program leaves an unclosed transaction scope")
    return steps


def _observations(program, steps):
    observed = set()
    for observation in _list(program["observations"], MAX_STEPS + 1):
        wire.fields(observation, {"step", "oracle"}, {"error"})
        identity, oracle = observation["step"], observation["oracle"]
        if identity in observed or identity not in steps and identity != "final":
            raise wire.FormatError("missing or duplicate observed effect")
        if oracle not in ("reference", "fixture-state", "native-parity", "exact-error"):
            raise wire.FormatError("unknown oracle obligation")
        if (identity == "final") != (oracle == "fixture-state"):
            raise wire.FormatError("fixture-state observation must name final state")
        if oracle == "exact-error":
            wire.fields(observation.get("error"), {"class", "cause"})
            parameters.validate(
                observation["error"]["class"],
                (
                    "ConstructionError",
                    "DecodeError",
                    "DatabaseError",
                    "LifecycleError",
                    "UnsupportedCapabilityError",
                ),
            )
            parameters.validate(observation["error"]["cause"], "string")
            if not observation["error"]["cause"]:
                raise wire.FormatError("expected errors require a specific cause")
        elif "error" in observation:
            raise wire.FormatError("error expectation requires the exact-error oracle")
        observed.add(identity)
    if observed != steps.keys() | {"final"}:
        raise wire.FormatError("every effect and final fixture state require an oracle")


def _reachable(nodes, steps):
    pending = [
        ref for step in steps.values() for ref in parameters.input_ids(step["inputs"])
    ]
    seen = set()
    while pending:
        identity = pending.pop()
        if identity not in seen:
            seen.add(identity)
            pending.extend(parameters.input_ids(nodes[identity]["inputs"]))
    if seen != nodes.keys():
        raise wire.FormatError("program contains instructions that no effect can reach")


# [spec:pgorm:def:generative.program]
# [spec:pgorm:req:generative.format]
def validate(value):
    _budget(value)
    wire.fields(
        value,
        {
            "version",
            "capability_version",
            "seed",
            "fixture",
            "binders",
            "nodes",
            "steps",
            "observations",
        },
    )
    if type(value["version"]) is not int or value["version"] != VERSION:
        raise wire.FormatError("unsupported program format version")
    if (
        type(value["capability_version"]) is not int
        or value["capability_version"] != catalog.VERSION
    ):
        raise wire.FormatError("incompatible instruction capability version")
    if type(value["seed"]) is not int or not 0 <= value["seed"] < 2**64:
        raise wire.FormatError("seed must be an unsigned 64-bit integer")
    baseline.render(value["fixture"])
    binders = _binders(value["binders"])
    nodes, types, results = _nodes(value, binders)
    steps = _steps(value, nodes, types, results)
    _observations(value, steps)
    _reachable(nodes, steps)
    return value


@dataclass(frozen=True, init=False)
class Program:
    """An immutable canonical artifact; callers receive detached decoded data."""

    encoded: str

    def __init__(self, data):
        if not isinstance(data, (str, bytes)) or len(data) > MAX_BYTES:
            raise wire.FormatError("program input must be bounded JSON text or bytes")
        try:
            value = json.loads(
                data, object_pairs_hook=_pairs, parse_constant=_nonfinite
            )
            encoded = json.dumps(
                value,
                ensure_ascii=False,
                sort_keys=True,
                separators=(",", ":"),
                allow_nan=False,
            )
            if len(encoded.encode("utf-8")) > MAX_BYTES:
                raise wire.FormatError("program exceeds its serialized byte budget")
            validate(value)
        except (
            ValueError,
            TypeError,
            KeyError,
            RecursionError,
            OverflowError,
        ) as error:
            raise wire.FormatError(str(error)) from error
        object.__setattr__(self, "encoded", encoded)

    @classmethod
    def from_dict(cls, value):
        try:
            return cls(json.dumps(value, ensure_ascii=False, allow_nan=False))
        except (ValueError, TypeError, RecursionError, UnicodeError) as error:
            raise wire.FormatError(str(error)) from error

    def data(self):
        return json.loads(self.encoded)

    @property
    def digest(self):
        return hashlib.sha256(self.encoded.encode("utf-8")).hexdigest()
