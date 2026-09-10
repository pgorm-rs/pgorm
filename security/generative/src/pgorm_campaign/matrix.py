"""Versioned coverage obligations, separated from observed execution evidence."""

from importlib.resources import files
import json

from . import catalog, wire


# [spec:pgorm:req:generative.matrix]
def load():
    value = json.loads(files(__package__).joinpath("matrix.json").read_text())
    if value["version"] != 1 or value["instruction_version"] != catalog.VERSION:
        raise wire.FormatError("coverage matrix does not match the instruction catalog")
    families = {item["id"] for item in value["families"]}
    if len(families) != len(value["families"]):
        raise wire.FormatError("duplicate coverage family")
    for name, operation in catalog.OPERATIONS.items():
        if operation.family not in families or not operation.rust:
            raise wire.FormatError(
                "operation has no coverage family or native path: " + name
            )
    kinds = {item["kind"] for item in value["value_matrix"]}
    if kinds != wire.SCALARS | {"enum"} or len(kinds) != len(value["value_matrix"]):
        raise wire.FormatError(
            "value/context matrix must enumerate every scalar and enum kind"
        )
    for item in value["value_matrix"]:
        if item["policy"] != "live" and not item.get("reason"):
            raise wire.FormatError("non-live value coverage requires a reason")
    excluded = set()
    for item in value["outside_runtime"]:
        if item["id"] in excluded or not item["reason"] or not item["rust"]:
            raise wire.FormatError(
                "unsupported/compile-only paths need unique identities and reasons"
            )
        excluded.add(item["id"])
    return value


def obligations():
    """The runner must satisfy these with observations, never scheduled labels."""
    value = load()
    required = {"operation." + name for name in catalog.OPERATIONS}
    required.update("effect." + name for name in catalog.EFFECTS)
    for family in value["families"]:
        required.update(
            family["id"] + "." + variation for variation in family["variations"]
        )
    for category in ("registered_entities", "registered_graphs", "registered_sources"):
        required.update(category + "." + name for name in value[category])
    for row in value["value_matrix"]:
        required.update(
            "value." + row["kind"] + "." + context
            for context in value["value_contexts"]
        )
        required.update(
            "array." + row["kind"] + "." + context
            for context in value["array_contexts"]
        )
    return frozenset(required)
