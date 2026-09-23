"""Refusals the binding documents, spelled exactly as it reports them.

An exact-error declaration names one of these, and the independent reference
has to arrive at the same refusal from its own model of the rule, so the text
lives here once rather than in both places. Each entry names the rule that
makes it a refusal rather than a defect.
"""

# `pipeline.errors+3`: an identifier carrying `"` or NUL is refused at
# `into_sql`, before prqlc sees it, rather than escaped.
UNQUOTABLE = (
    "identifier `{}` contains a double quote or NUL byte, which no quoted "
    "PostgreSQL identifier can carry; rename it"
)

# pgorm-python STATEMENTS.md: "An insert with no rows raises an error;
# `default_values()` explicitly requests one default row." An empty batch is
# refused at compilation, before any statement reaches the server.
EMPTY_INSERT = "insert requires rows or explicit default_values"

# pgorm-python errors.rs: a value tokio-postgres cannot encode while building
# the bind message is a ConstructionError carrying the driver's own top-level
# text, which names the parameter's zero-based position.
UNSERIALIZABLE = "error serializing parameter {}"

# The largest u64 an int8 carrier can hold (`exec.cursor.binding-coerce+2`).
CARRIED_UNSIGNED = 2**63 - 1


def unsigned_overflow(value):
    """Whether a tagged value is, or holds, a u64 no int8 carrier can take."""
    tag = value["type"]
    if value["sql_null"]:
        return False
    if tag["kind"] == "array":
        return any(unsigned_overflow(item) for item in value["data"])
    return tag["kind"] == "u64" and int(value["data"]) > CARRIED_UNSIGNED


def unquotable(name):
    """Whether the pipeline refuses this identifier outright."""
    return '"' in name or "\0" in name


__all__ = [
    "CARRIED_UNSIGNED",
    "EMPTY_INSERT",
    "UNQUOTABLE",
    "UNSERIALIZABLE",
    "unquotable",
    "unsigned_overflow",
]
