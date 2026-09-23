"""Deliberate rejections, each labelled with the exact cause it must produce.

An invalid program is a claim about where a line is drawn — by PostgreSQL, or
by pgorm and its binding before anything is sent. Every rule declares the
precise error, a SQLSTATE or the binding's documented refusal text, so an
arbitrary failure can never satisfy it, and the independent reference has to
reach the same rejection from its own model of the rule. A valid-mode failure
is never relabelled as one of these.
"""

from .grammar_pipeline import Pipeline
from .grammar_sequence import account_row, accounts, tenant_guard
from .refusals import UNQUOTABLE


def _database(sqlstate):
    return {"class": "DatabaseError", "cause": "sqlstate:" + sqlstate}


def _division(state):
    left = state.constant("i32", state.choices.integer(1, 100))
    right = state.constant("i32", 0)
    expression = state.binary(left, right, "div")
    query = state.node(
        "select",
        {"columns": [expression.node, state.constant("i64", state.index).node]},
    )
    state.fetch(query, error=_database("22012"))


def _not_null(state):
    table = accounts(state)
    query = state.node("update", {"table": table})
    query = state.node(
        "update.set",
        {"query": query, "value": state.value("text", None, sql_null=True)},
        {"column": "name"},
    )
    query = state.node(
        "write.filter",
        {
            "query": query,
            "predicate": tenant_guard(state, table, state.value("i32", 1)),
        },
    )
    state.author.effect("execute", {"query": query}, error=_database("23502"))


def _duplicate(state):
    table = accounts(state)
    columns, values = account_row(state, 1)
    query = state.node("insert", {"table": table}, {"columns": columns})
    query = state.node("insert.row", {"query": query, "values": values})
    state.author.effect("execute", {"query": query}, error=_database("23505"))


def _unquotable(state):
    """A pipeline alias carrying `"` is refused at `into_sql`, not escaped.

    `pipeline.errors+3` refuses rather than escapes because prqlc does the
    quoting and which prqlc a consumer links decides what the quote becomes.
    Exactly one identifier in the program carries a quote, so the refusal can
    only name that one.
    """
    alias = state.name("origin", quote=True)
    query = Pipeline(state, alias)
    query.project()
    state.fetch(
        query.query,
        error={"class": "ConstructionError", "cause": UNQUOTABLE.format(alias)},
    )


RULES = {
    "division": _division,
    "not-null": _not_null,
    "duplicate": _duplicate,
    "unquotable-identifier": _unquotable,
}


# [spec:pgorm:req:generative.grammar]
def rejection(state):
    """An intentional rejection; never relabel a valid-mode failure."""
    rule = state.choices.take(tuple(RULES))
    RULES[rule](state)
    return rule


__all__ = ["RULES", "rejection"]
