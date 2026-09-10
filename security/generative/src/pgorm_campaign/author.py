"""Author portable programs without importing or invoking the subject library."""

from . import baseline, wire
from .program import Program


class Author:
    def __init__(self, *, seed=0, fixture=None):
        self.seed, self.fixture = seed, fixture or baseline.default()
        self.nodes, self.steps, self.binders, self.observations = [], [], [], []

    def node(self, operation, inputs=None, data=None, *, scope="root"):
        identity = "n" + str(len(self.nodes))
        self.nodes.append(
            {
                "id": identity,
                "op": operation,
                "inputs": inputs or {},
                "data": data or {},
                "scope": scope,
            }
        )
        return identity

    def value(self, kind, data=None, *, sql_null=False):
        if isinstance(kind, str) and kind in wire.INTEGER_BITS and not sql_null:
            data = str(data)
        return self.node(
            "value", data={"value": wire.scalar(kind, data, sql_null=sql_null)}
        )

    def effect(self, operation, inputs=None, data=None, *, scope="root", error=None):
        identity = "s" + str(len(self.steps))
        self.steps.append(
            {
                "id": identity,
                "op": operation,
                "inputs": inputs or {},
                "data": data or {},
                "scope": scope,
            }
        )
        self.observations.append(
            {"step": identity, "oracle": "reference"}
            if error is None
            else {"step": identity, "oracle": "exact-error", "error": error}
        )
        return identity

    def fetch(self, query, *, ordered=False):
        return self.effect(
            "fetch", {"query": query}, {"mode": "all", "ordered": ordered}
        )

    def finish(self):
        return Program.from_dict(
            {
                "version": 1,
                "capability_version": 1,
                "seed": self.seed,
                "fixture": self.fixture,
                "nodes": self.nodes,
                "steps": self.steps,
                "binders": self.binders,
                "observations": self.observations
                + [{"step": "final", "oracle": "fixture-state"}],
            }
        )
