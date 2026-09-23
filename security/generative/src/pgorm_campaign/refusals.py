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


def unquotable(name):
    """Whether the pipeline refuses this identifier outright."""
    return '"' in name or "\0" in name


__all__ = ["EMPTY_INSERT", "UNQUOTABLE", "unquotable"]
