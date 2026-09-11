"""Reduce a failing generated program while preserving its original failure."""

import asyncio
import copy
import time
from dataclasses import dataclass, field

from . import baseline, wire
from .parameters import input_ids
from .program import Program

VERSION = 1

# Verdicts that describe a failure worth reducing. A passing or merely expected
# rejection has no defect to preserve, so freezing a predicate from one is an
# error rather than a silently empty search.
FAILING = ("defect", "incomplete")

# Value payloads this module knows how to make smaller without changing the
# declared type identity. Kinds whose exact spelling carries the defect (floats
# as IEEE bits, decimals, temporal precision, enum labels, UUIDs) are left
# alone: a "simpler" spelling there would be a different test, not a smaller one.
SIMPLER = {
    "bool": (False,),
    "text": ("",),
    "char": ("a",),
    "bytes": ([],),
    "json": ({},),
    "i8": ("0",),
    "i16": ("0",),
    "i32": ("0",),
    "i64": ("0",),
    "u32": ("0",),
    "u64": ("0",),
}


class ShrinkError(ValueError):
    """A shrink attempt was asked for something it cannot honestly do."""


# [spec:pgorm:req:generative.shrink]
@dataclass(frozen=True)
class Budget:
    """Bounds every shrink run records and reports, whether or not it finishes."""

    candidates: int = 240
    seconds: float = 1800.0
    passes: int = 12
    timeout: float = 10.0
    offered: int = 20000

    def __post_init__(self):
        if self.candidates < 1 or self.passes < 1 or self.offered < 1:
            raise ShrinkError("shrink budgets must allow at least one attempt")
        if self.seconds <= 0 or self.timeout <= 0:
            raise ShrinkError("shrink deadlines must be positive")

    def data(self):
        return {
            "candidates": self.candidates,
            "seconds": self.seconds,
            "passes": self.passes,
            "timeout": self.timeout,
            "offered": self.offered,
        }


# [spec:pgorm:req:generative.shrink]
@dataclass(frozen=True)
class Predicate:
    """The original failure, frozen so a reduction cannot quietly replace it."""

    status: str
    kind: str
    signature: tuple
    witness: tuple

    @classmethod
    def freeze(cls, report):
        """Derive the predicate from a completed independent baseline run."""
        status = report.get("status")
        if status not in FAILING:
            raise ShrinkError(
                "shrinking requires a recorded failure, not: " + str(status)
            )
        if status == "defect":
            differences = [
                item for item in report.get("comparisons", ()) if not item["equal"]
            ]
            if not differences:
                raise ShrinkError("a defect report carries no differing comparison")
            first = differences[0]
            return cls(
                "defect",
                "difference",
                (first["oracle"], first["reason"]),
                witness(report, first["step"]),
            )
        error = report.get("error")
        if not isinstance(error, dict) or "class" not in error or "cause" not in error:
            raise ShrinkError("an incomplete report carries no recorded error")
        return cls(
            "incomplete", "error", (error["class"], error["cause"]), ("report-error",)
        )

    def holds(self, report):
        """Report whether a candidate still exhibits the original failure.

        A comparison reason alone is too coarse to pin a defect: "observation
        categories differ" covers both a PostgreSQL rejection and a builder that
        never compiled its query. The subject's own account of the failing step
        is compared too, so a reduction that trades a database error for an
        earlier construction error is rejected rather than adopted.
        """
        if report.get("status") != self.status:
            return False
        if self.kind == "difference":
            return any(
                (item["oracle"], item["reason"]) == self.signature
                and witness(report, item["step"]) == self.witness
                for item in report.get("comparisons", ())
                if not item["equal"]
            )
        error = report.get("error")
        if not isinstance(error, dict):
            return False
        return (error.get("class"), error.get("cause")) == self.signature

    def data(self):
        names = (
            ("oracle", "reason") if self.kind == "difference" else ("class", "cause")
        )
        return {
            "status": self.status,
            "kind": self.kind,
            **dict(zip(names, self.signature)),
            "witness": list(self.witness),
        }


def witness(report, step):
    """Signature of what the subject itself did at a step, beyond the verdict.

    Error causes carry values that shrinking is entitled to change, so the
    identity is the observation category plus the error class and SQLSTATE —
    stable under reduction, but distinct across genuinely different failures.
    """
    for entry in report.get("subject", {}).get("steps", ()):
        if entry["id"] != step:
            continue
        observation = entry.get("inspection_probe", entry.get("observation")) or {}
        if observation.get("kind") == "error":
            return (
                "error",
                observation.get("class"),
                observation.get("sqlstate"),
            )
        return ("observation", observation.get("kind"))
    return ("final",) if step == "final" else ("absent",)


def _step_ids(data):
    return {step["id"] for step in data["steps"]}


def _observations(data):
    """Rebuild the observation list so it names exactly the surviving steps."""
    declared = {item["step"]: item for item in data["observations"]}
    kept = [declared[step["id"]] for step in data["steps"] if step["id"] in declared]
    final = declared.get("final", {"step": "final", "oracle": "fixture-state"})
    return kept + [final]


def _repair(data):
    """Drop whatever a deletion left dangling, then prune what nothing reaches.

    Removing a step orphans the nodes that read its rows; removing a node
    orphans everything built on it. Both cascade, so this runs to a fixed point
    before the candidate is offered for validation.
    """
    while True:
        steps = _step_ids(data)
        alive = {
            node["id"]
            for node in data["nodes"]
            if node["data"].get("step") in steps or "step" not in node["data"]
        }
        changed = True
        while changed:
            changed = False
            for node in data["nodes"]:
                if node["id"] not in alive:
                    continue
                if any(ref not in alive for ref in input_ids(node["inputs"])):
                    alive.discard(node["id"])
                    changed = True
        kept = [
            step
            for step in data["steps"]
            if all(ref in alive for ref in input_ids(step["inputs"]))
        ]
        if len(alive) == len(data["nodes"]) and len(kept) == len(data["steps"]):
            break
        data["nodes"] = [node for node in data["nodes"] if node["id"] in alive]
        data["steps"] = kept
    nodes = {node["id"]: node for node in data["nodes"]}
    pending = [ref for step in data["steps"] for ref in input_ids(step["inputs"])]
    seen = set()
    while pending:
        current = pending.pop()
        if current not in seen:
            seen.add(current)
            pending.extend(input_ids(nodes[current]["inputs"]))
    data["nodes"] = [node for node in data["nodes"] if node["id"] in seen]
    data["binders"] = [binder for binder in data["binders"] if binder["owner"] in seen]
    data["observations"] = _observations(data)
    return data


def _candidate(data):
    """Validate a rewritten document, discarding anything the format rejects."""
    try:
        return Program.from_dict(_repair(data))
    except (wire.FormatError, ValueError, KeyError, TypeError):
        return None


def _transactions(steps):
    """Pair each begin with the commit or rollback that closes it."""
    stack, pairs = [], []
    for index, step in enumerate(steps):
        if step["op"] == "begin":
            stack.append((index, step["data"]["child"], step["scope"]))
        elif step["op"] in ("commit", "rollback") and stack:
            start, child, parent = stack.pop()
            pairs.append((start, index, child, parent))
    return pairs


def _balanced(steps):
    depth = 0
    for step in steps:
        if step["op"] == "begin":
            depth += 1
        elif step["op"] in ("commit", "rollback"):
            depth -= 1
            if depth < 0:
                return False
    return depth == 0


def _drop_steps(data, indexes):
    data["steps"] = [
        step for index, step in enumerate(data["steps"]) if index not in indexes
    ]
    return data


def _sequence_reductions(data):
    """Reduce the operation sequence: shorter prefixes, then single effects."""
    steps = data["steps"]
    for keep in range(len(steps) - 1, 0, -1):
        if _balanced(steps[:keep]):
            candidate = copy.deepcopy(data)
            candidate["steps"] = candidate["steps"][:keep]
            yield (
                "first " + str(keep) + " of " + str(len(steps)) + " effects",
                candidate,
            )
    for index, step in enumerate(steps):
        if step["op"] in ("begin", "commit", "rollback"):
            continue
        yield "drop effect " + step["id"], _drop_steps(copy.deepcopy(data), {index})


def _transaction_reductions(data):
    """Unwrap a transaction scope, re-homing its effects onto the parent."""
    for start, end, child, parent in _transactions(data["steps"]):
        candidate = _drop_steps(copy.deepcopy(data), {start, end})
        for step in candidate["steps"]:
            if step["scope"] == child:
                step["scope"] = parent
        yield "unwrap transaction " + child, candidate


def _structure_reductions(data):
    """Collapse expression and pipeline structure by bypassing one instruction.

    Every reference to a node is redirected to one of that node's own inputs,
    which removes a predicate wrapper, a pipeline stage or a join in one edit.
    Whether the replacement types check is left to format validation.
    """
    for node in data["nodes"]:
        for key, value in node["inputs"].items():
            for reference in value if isinstance(value, list) else [value]:
                candidate = copy.deepcopy(data)
                _redirect(candidate, node["id"], reference)
                yield (
                    "bypass " + node["op"] + " " + node["id"] + " to " + key,
                    candidate,
                )


def _redirect(data, source, target):
    for node in data["nodes"]:
        node["inputs"] = _rewrite(node["inputs"], source, target)
    for step in data["steps"]:
        step["inputs"] = _rewrite(step["inputs"], source, target)
    data["nodes"] = [node for node in data["nodes"] if node["id"] != source]


def _rewrite(inputs, source, target):
    rewritten = {}
    for key, value in inputs.items():
        if isinstance(value, list):
            rewritten[key] = [target if ref == source else ref for ref in value]
        else:
            rewritten[key] = target if value == source else value
    return rewritten


def _value_reductions(data):
    """Reduce payloads in place, keeping each declared type identity intact."""
    for node in data["nodes"]:
        if node["op"] != "value":
            continue
        value = node["data"]["value"]
        if value["sql_null"]:
            continue
        kind = value["type"]["kind"]
        for payload in SIMPLER.get(kind, ()):
            if value["data"] == payload:
                continue
            candidate = copy.deepcopy(data)
            _payload(candidate, node["id"])["data"] = payload
            yield "simplify " + kind + " value " + node["id"], candidate
        if kind == "array" and value["data"]:
            for keep in (0, len(value["data"]) // 2):
                if keep == len(value["data"]):
                    continue
                candidate = copy.deepcopy(data)
                items = _payload(candidate, node["id"])["data"]
                _payload(candidate, node["id"])["data"] = items[:keep]
                yield (
                    "keep " + str(keep) + " array elements in " + node["id"],
                    candidate,
                )


def _payload(data, identity):
    for node in data["nodes"]:
        if node["id"] == identity:
            return node["data"]["value"]
    raise ShrinkError("value instruction disappeared during reduction")


def _fixture_reductions(data):
    """Reduce the declared baseline: fewer rows first, then whole tables."""
    tables = data["fixture"]["tables"]
    for index, table in enumerate(tables):
        rows = table["rows"]
        for keep in sorted({0, len(rows) // 2}):
            if keep >= len(rows):
                continue
            candidate = copy.deepcopy(data)
            candidate["fixture"]["tables"][index]["rows"] = rows[:keep]
            yield (
                "keep "
                + str(keep)
                + " rows in "
                + table["schema"]
                + "."
                + table["name"],
                candidate,
            )
    for index, table in enumerate(tables):
        if len(tables) == 1:
            break
        candidate = copy.deepcopy(data)
        del candidate["fixture"]["tables"][index]
        yield "drop table " + table["schema"] + "." + table["name"], candidate


REDUCTIONS = (
    ("sequence", _sequence_reductions),
    ("transaction", _transaction_reductions),
    ("structure", _structure_reductions),
    ("value", _value_reductions),
    ("fixture", _fixture_reductions),
)


# [spec:pgorm:req:generative.shrink]
@dataclass(frozen=True)
class Result:
    """Everything a reduction run learned, including what it never got to try."""

    original: Program
    best: Program
    predicate: Predicate
    budget: Budget
    attempts: tuple = field(default_factory=tuple)
    interrupted: bool = False
    exhausted: str = ""
    seconds: float = 0.0

    def report(self):
        original = self.original.data()
        best = self.best.data()
        return {
            "version": VERSION,
            "predicate": self.predicate.data(),
            "budget": self.budget.data(),
            "original": {
                "program_sha256": self.original.digest,
                "nodes": len(original["nodes"]),
                "steps": len(original["steps"]),
                "fixture_sha256": baseline.digest(original["fixture"]),
            },
            "best": {
                "program_sha256": self.best.digest,
                "nodes": len(best["nodes"]),
                "steps": len(best["steps"]),
                "fixture_sha256": baseline.digest(best["fixture"]),
            },
            "reduced": self.best.digest != self.original.digest,
            "attempts": list(self.attempts),
            "offered": len(self.attempts),
            "executed": sum(
                1
                for item in self.attempts
                if item["outcome"] in ("accepted", "rejected")
            ),
            "invalid": sum(1 for item in self.attempts if item["outcome"] == "invalid"),
            "accepted": sum(
                1 for item in self.attempts if item["outcome"] == "accepted"
            ),
            "interrupted": self.interrupted,
            "exhausted": self.exhausted,
            "seconds": self.seconds,
            "globally_minimal": False,
            "claim": (
                "a locally reduced reproducer that still satisfies the recorded "
                "predicate; no claim of global minimality is made"
            ),
        }


class _Deadline:
    def __init__(self, budget):
        self.budget = budget
        self.started = time.monotonic()
        self.executed = 0
        self.offered = 0

    def elapsed(self):
        return time.monotonic() - self.started

    def exhausted(self):
        if self.executed >= self.budget.candidates:
            return "candidate budget"
        if self.offered >= self.budget.offered:
            return "offered budget"
        if self.elapsed() >= self.budget.seconds:
            return "time budget"
        return ""


# [spec:pgorm:req:generative.shrink]
async def reduce(checker, program, *, budget=None, report=None, observer=None):
    """Search for a smaller program with the same failure, from its own baseline.

    Every candidate is validated before it runs and executed through `checker`,
    which restores the declared fixture on both databases first, so a candidate
    never inherits the previous attempt's state. A candidate is accepted only
    when the frozen predicate still holds: a candidate that merely fails some
    other way — malformed, unsupported, or a fixture error — is recorded and
    discarded rather than adopted as the new reproducer.
    """
    if not isinstance(program, Program):
        raise ShrinkError("shrinking requires a validated Program")
    budget = budget or Budget()
    if report is None:
        report = await checker.run(program, timeout=budget.timeout)
    predicate = Predicate.freeze(report)
    deadline = _Deadline(budget)
    attempts, seen = [], {program.digest}
    best, interrupted = program, False
    try:
        for _ in range(budget.passes):
            best, progressed = await _pass(
                checker, best, predicate, budget, deadline, attempts, seen, observer
            )
            if not progressed or deadline.exhausted():
                break
    except (asyncio.CancelledError, KeyboardInterrupt):
        interrupted = True
    return Result(
        original=program,
        best=best,
        predicate=predicate,
        budget=budget,
        attempts=tuple(attempts),
        interrupted=interrupted,
        exhausted=deadline.exhausted(),
        seconds=deadline.elapsed(),
    )


async def _pass(checker, best, predicate, budget, deadline, attempts, seen, observer):
    """Try every reduction once against the current best; keep the first win."""
    progressed = False
    for name, reduction in REDUCTIONS:
        restart = True
        while restart:
            restart = False
            for description, rewritten in reduction(best.data()):
                if deadline.exhausted():
                    return best, progressed
                deadline.offered += 1
                candidate = _candidate(rewritten)
                if candidate is None:
                    attempts.append(_attempt(name, description, None, "invalid", ""))
                    continue
                if candidate.digest in seen:
                    continue
                seen.add(candidate.digest)
                deadline.executed += 1
                outcome = await checker.run(candidate, timeout=budget.timeout)
                held = predicate.holds(outcome)
                attempts.append(
                    _attempt(
                        name,
                        description,
                        candidate,
                        "accepted" if held else "rejected",
                        outcome.get("status", ""),
                    )
                )
                if observer is not None:
                    observer(attempts[-1])
                if held:
                    best, progressed, restart = candidate, True, True
                    break
    return best, progressed


def _attempt(reduction, description, candidate, outcome, status):
    return {
        "reduction": reduction,
        "description": description,
        "program_sha256": None if candidate is None else candidate.digest,
        "nodes": None if candidate is None else len(candidate.data()["nodes"]),
        "steps": None if candidate is None else len(candidate.data()["steps"]),
        "outcome": outcome,
        "status": status,
    }
