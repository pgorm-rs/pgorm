"""Generate a temporal key's versions and a relation's PERIOD, deferral and NOT ENFORCED.

`campaign.Room` is keyed `(id, valid_at WITHOUT OVERLAPS)`: a room holds a
version per period, none overlapping, and a write by key names one version by
equality on the period too. `campaign.Stay` references a room over its own
stay, a temporal foreign key the server checks at commit, and an account the
server never checks. The two tables are appended to the program's own fixture,
so a program that never names them keeps the shared baseline.
"""

from . import baseline
from .grammar_state import Field, Source

ROOM_ROWS = (
    (1, ((1, 1), (2, 1)), 100),
    (1, ((2, 1), (3, 1)), 120),
    (2, ((1, 1), (2, 1)), 80),
)
# Stay 1 overlaps both of room 1's versions; stay 3 names an account that does
# not exist, which only the NOT ENFORCED key admits.
STAY_ROWS = (
    (1, 1, 1, ((1, 20), (2, 10))),
    (2, 2, 2, ((1, 5), (1, 9))),
    (3, 1, 99, ((2, 14), (2, 16))),
)
GRAPHS = ("StayRooms", "StayGuests")


def days(start, end):
    """`[start, end)` in 2026, each a `(month, day)` pair, as a range payload."""
    return {
        "lower": "2026-{:02d}-{:02d}".format(*start),
        "upper": "2026-{:02d}-{:02d}".format(*end),
        "bounds": "[)",
    }


def _text(period):
    value = days(*period)
    return "[" + value["lower"] + "," + value["upper"] + ")"


def tables(state):
    """Append rooms and stays to the program's fixture, once."""
    declared = state.author.fixture["tables"]
    if any(
        table["schema"] == "fixture" and table["name"] == "rooms" for table in declared
    ):
        return
    declared.append(
        {
            "schema": "fixture",
            "name": "rooms",
            "columns": [
                baseline.column("id", "i32", primary=True),
                baseline.column("valid_at", "daterange", primary=True),
                baseline.column("rate", "i32"),
            ],
            "rows": [[room, _text(period), rate] for room, period, rate in ROOM_ROWS],
            "without_overlaps": "valid_at",
        }
    )
    declared.append(
        {
            "schema": "fixture",
            "name": "stays",
            "columns": [
                baseline.column("id", "i32", primary=True),
                baseline.column("room_id", "i32"),
                baseline.column("guest_id", "i32"),
                baseline.column("during", "daterange"),
            ],
            "rows": [
                [stay, room, guest, _text(period)]
                for stay, room, guest, period in STAY_ROWS
            ],
            "foreign_keys": [
                {
                    "columns": ["room_id"],
                    "table": ["fixture", "rooms"],
                    "references": ["id"],
                    "period": ["during", "valid_at"],
                    "enforced": True,
                    "deferred": True,
                },
                {
                    "columns": ["guest_id"],
                    "table": ["fixture", "accounts"],
                    "references": ["id"],
                    "period": None,
                    "enforced": False,
                    "deferred": False,
                },
            ],
        }
    )


def fields(state, table, *, optional=False):
    definition = next(
        item
        for item in state.author.fixture["tables"]
        if item["schema"] == "fixture" and item["name"] == table
    )
    return tuple(
        Field(column["name"], column["kind"], optional or column["nullable"])
        for column in definition["columns"]
    )


def entity_source(state, name):
    tables(state)
    node = state.node("entity", data={"name": "campaign." + name})
    return Source(node, name, fields(state, name.lower() + "s"), "entity")


def _equal(state, source, column, value):
    return state.node(
        "entity.predicate",
        {"entity": source.node, "value": value},
        {"column": column, "operator": "eq"},
    )


def _insert(state, source, assignments, scope):
    active = state.node("entity.active", {"entity": source.node})
    for column, value in assignments:
        active = state.node(
            "active.set",
            {"model": active, "value": value},
            {"column": column, "state": "set"},
        )
    return state.author.effect(
        "active.write", {"model": active}, {"method": "insert"}, scope=scope
    )


def _version_by_key(state, source, scope):
    """Read one of the fixture's versions by its whole key, period included."""
    room, period, _ = state.choices.take(ROOM_ROWS)
    query = state.node("entity.find", {"entity": source.node})
    for column, value in (
        ("id", state.value("i32", room)),
        ("valid_at", state.value("daterange", days(*period))),
    ):
        query = state.node(
            "entity.filter",
            {"query": query, "predicate": _equal(state, source, column, value)},
        )
    return state.fetch(query, scope=scope)


# [spec:pgorm:req:generative.grammar]
def temporal_key(state):
    """A room version read or written by its whole key, then updated and
    perhaps deleted by it: only the version the period names changes."""
    source = entity_source(state, "Room")
    scope = "root"
    if state.choices.take((False, True)):
        state.author.effect(
            "begin",
            data={"child": "tx", "mode": "read_write", "isolation": "read_committed"},
        )
        scope = "tx"
    inserted = state.choices.take((False, True))
    if inserted:
        # A new version after room 1's, or room 2's February.
        room, period = state.choices.take(
            ((1, ((3, 1), (4, 1))), (2, ((2, 1), (3, 1))))
        )
        step = _insert(
            state,
            source,
            (
                ("id", state.value("i32", room)),
                ("valid_at", state.value("daterange", days(*period))),
                ("rate", state.value("i32", state.choices.integer(1, 400))),
            ),
            scope,
        )
    else:
        step = _version_by_key(state, source, scope)
    for _ in range(state.choices.integer(1, 2)):
        model = state.node("entity.result", data={"step": step, "row": 0})
        active = state.node("entity.into_active", {"model": model})
        active = state.node(
            "active.set",
            {
                "model": active,
                "value": state.value("i32", state.choices.integer(1, 400)),
            },
            {"column": "rate", "state": "set"},
        )
        step = state.author.effect(
            "active.write", {"model": active}, {"method": "update"}, scope=scope
        )
    # Only the version this program wrote is deleted: every version the
    # fixture holds is referenced by a stay, which the deferred temporal key
    # would refuse at commit.
    if inserted and state.choices.take((False, True)):
        model = state.node("entity.result", data={"step": step, "row": 0})
        active = state.node("entity.into_active", {"model": model})
        state.author.effect(
            "active.write", {"model": active}, {"method": "delete"}, scope=scope
        )
    if scope != "root":
        state.author.effect(state.choices.take(("commit", "rollback")), scope=scope)


# [spec:pgorm:req:generative.grammar]
def deferred_key(state):
    """A stay written before the room version it references, in one
    transaction: the temporal key's check waits for the commit."""
    stay = entity_source(state, "Stay")
    room = entity_source(state, "Room")
    state.author.effect(
        "begin",
        data={"child": "tx", "mode": "read_write", "isolation": "read_committed"},
    )
    identity = 10 + state.index % 80
    period = state.choices.take((((5, 1), (5, 5)), ((6, 10), (6, 12))))
    _insert(
        state,
        stay,
        (
            ("id", state.value("i32", 100 + state.index)),
            ("room_id", state.value("i32", identity)),
            ("guest_id", state.value("i32", state.choices.take((1, 4, 99)))),
            ("during", state.value("daterange", days(*period))),
        ),
        "tx",
    )
    _insert(
        state,
        room,
        (
            ("id", state.value("i32", identity)),
            (
                "valid_at",
                state.value("daterange", days(period[0], (period[1][0] + 1, 1))),
            ),
            ("rate", state.value("i32", state.choices.integer(1, 400))),
        ),
        "tx",
    )
    state.author.effect("commit", scope="tx")


def graph_sources(state, name, query, alias):
    """The root stay and its joined room or guest, both decoded optional."""
    tables(state)
    target = "rooms" if name == "StayRooms" else "accounts"
    joined = fields(state, target, optional=True)
    if target == "accounts":
        joined = tuple(
            field
            for field in joined
            if isinstance(field.kind, str) and field.name != "tags"
        )
    return [
        Source(query, "Stay", fields(state, "stays"), "graph"),
        Source(query, alias, joined, "graph", 1),
    ]
