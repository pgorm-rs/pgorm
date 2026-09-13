"""Coverage computed from what a run observed, never from what it scheduled.

A scheduled family is a plan; a constructed instruction with a recorded native
path and an oracle decision is evidence. Only the second discharges an
obligation here, so a class that generated a thousand programs and executed
none of them satisfies nothing.

The full coverage matrix is evaluated on every run regardless of what the
profile requires, and the outstanding obligations are recorded. A smoke run
therefore cannot be read as a full run by omission: the gap is in its report.
"""

from . import matrix
from .catalog import EFFECTS, OPERATIONS

# Contexts the instruction catalog itself names, so attribution is read off the
# declared instruction rather than guessed from how a value looked.
CONTEXTS = {
    "expr.value": None,
    "pipeline.value": "literal",
    "pipeline.bind": "bound",
    "result.value": "result-decode",
}

REGISTRATION_CATEGORIES = (
    "registered_entities",
    "registered_graphs",
    "registered_sources",
)


def _constructed(report):
    """Node identities the subject actually built through a native path."""
    subject = report.get("subject") or {}
    return {
        event["id"]
        for event in subject.get("trace", ())
        if event.get("status") == "constructed" and event.get("native_paths")
    }


def _operations(program, report):
    nodes = {node["id"]: node for node in program["nodes"]}
    return {
        "operation." + nodes[identity]["op"]
        for identity in _constructed(report)
        if identity in nodes and nodes[identity]["op"] in OPERATIONS
    }


def _effects(program, report):
    """An effect counts once the subject dispatched it, error included."""
    declared = {step["id"]: step["op"] for step in program["steps"]}
    subject = report.get("subject") or {}
    tokens = set()
    for step in subject.get("steps", ()):
        name = declared.get(step.get("id"))
        if name in EFFECTS and step.get("status") in ("observed", "error"):
            tokens.add("effect." + name)
    return tokens


def _payload(nodes, reference):
    node = nodes.get(reference)
    if node is None or node["op"] != "value":
        return None
    return node["data"]["value"]


def _array_tokens(value, context):
    element = value["type"]["element"]["kind"]
    tokens = {"array." + element + "." + context} if context == "bound" else set()
    if value["sql_null"]:
        tokens.add("array." + element + ".sql-null")
    elif not value["data"]:
        tokens.add("array." + element + ".empty")
    elif any(item.get("sql_null") for item in value["data"]):
        tokens.add("array." + element + ".nullable-elements")
    return tokens


def _value_tokens(value, context):
    kind = value["type"]["kind"]
    if kind == "array":
        return _array_tokens(value, context)
    tokens = {"value." + kind + "." + context}
    if value["sql_null"] and context != "result-decode":
        tokens.add("value." + kind + ".typed-null")
    return tokens


def _values(program, report):
    """Attribute each constructed value instruction to its declared context."""
    nodes = {node["id"]: node for node in program["nodes"]}
    tokens = set()
    for identity in _constructed(report):
        node = nodes.get(identity)
        if node is None or node["op"] not in CONTEXTS:
            continue
        if node["op"] == "result.value":
            kind = node["data"]["type"]["kind"]
            tokens.add("value." + kind + ".result-decode")
            continue
        context = CONTEXTS[node["op"]] or node["data"]["mode"]
        value = _payload(nodes, node["inputs"]["value"])
        if value is not None:
            tokens |= _value_tokens(value, context)
    return tokens


def _registrations(report, registry):
    subject = report.get("subject") or {}
    tokens = set()
    for event in subject.get("trace", ()):
        if event.get("status") != "constructed":
            continue
        name = (event.get("observation") or {}).get("registration")
        for category in REGISTRATION_CATEGORIES:
            if name in registry.get(category, ()):
                tokens.add(category + "." + name)
    return tokens


# [spec:pgorm:req:generative.verdict]
def observed(program, report, *, registry=None):
    """Every obligation token this one executed program actually discharged."""
    registry = matrix.load() if registry is None else registry
    return (
        _operations(program, report)
        | _effects(program, report)
        | _values(program, report)
        | _registrations(report, registry)
    )


# [spec:pgorm:req:generative.profiles]
def required(profile, plan):
    """The obligations the profile itself declared, resolved against the plan."""
    selector = profile.document["coverage"]["obligations"]
    if selector == "full-matrix":
        return frozenset(matrix.obligations()) | _scheduled_tokens(plan)
    if selector == "scheduled-families":
        return _scheduled_tokens(plan)
    raise ValueError("unknown coverage obligation selector: " + selector)


def _scheduled_tokens(plan):
    """Each live family and each included class must show observed evidence."""
    tokens = {"class." + item.run_class for item in plan.items}
    tokens |= {
        "family." + item.family
        for item in plan.items
        if item.family and item.run_class in ("runtime", "invalid")
    }
    return frozenset(tokens)


# [spec:pgorm:req:generative.verdict]
# [spec:pgorm:req:generative.artifacts]
def assess(profile, plan, tokens):
    """Decide declared coverage and record the full-matrix gap either way."""
    tokens = frozenset(tokens)
    declared = required(profile, plan)
    missing = sorted(declared - tokens)
    complete = frozenset(matrix.obligations())
    outstanding = sorted(complete - tokens)
    return {
        "obligations": profile.document["coverage"]["obligations"],
        "claim": profile.document["coverage"]["claim"],
        "declared_total": len(declared),
        "declared_satisfied": len(declared & tokens),
        "declared_missing": missing,
        "satisfied": not missing,
        "observed_total": len(tokens),
        "full_matrix": {
            "total": len(complete),
            "satisfied": len(complete & tokens),
            "outstanding": len(outstanding),
            "outstanding_sample": outstanding[:64],
            "claimed": profile.document["coverage"]["obligations"] == "full-matrix",
        },
    }


__all__ = ["CONTEXTS", "assess", "observed", "required"]
