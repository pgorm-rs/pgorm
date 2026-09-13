"""Turn a validated profile into the exact, countable work a run must perform.

The schedule is built before anything executes so "what was scheduled" and
"what was recorded" are two independently derived lists rather than one list
observed twice. Work that never produced a record is then a difference, not an
absence nobody can see.

Every item names its run class. Nothing in this module ever produces a total
across classes: a constructed program and an oracle-decided program are
different claims and are counted in different fields from here onwards.
"""

from dataclasses import dataclass, field

from . import profiles
from .control_catalog import catalog
from .grammar import FAMILIES


class PlanError(ValueError):
    """A profile cannot be turned into an honestly countable schedule."""


# [spec:pgorm:req:generative.profiles]
@dataclass(frozen=True)
class Item:
    """One scheduled unit of work, with the seed and worker it will use."""

    id: str
    run_class: str
    ordinal: int
    worker: int | None = None
    seed: int | None = None
    index: int | None = None
    family: str | None = None
    mode: str | None = None
    control_id: str | None = None
    expected: str = "pass"

    def data(self):
        return {
            "id": self.id,
            "run_class": self.run_class,
            "ordinal": self.ordinal,
            "worker": self.worker,
            "seed": self.seed,
            "index": self.index,
            "family": self.family,
            "mode": self.mode,
            "control_id": self.control_id,
            "expected": self.expected,
        }


@dataclass(frozen=True)
class Plan:
    """The whole schedule, addressable by class and by worker."""

    profile: str
    items: tuple = field(default_factory=tuple)

    def by_class(self, name):
        return tuple(item for item in self.items if item.run_class == name)

    def by_worker(self, worker):
        return tuple(item for item in self.items if item.worker == worker)

    def counts(self):
        return {
            name: len(self.by_class(name))
            for name in profiles.CLASSES
            if self.by_class(name)
        }

    def data(self):
        return {
            "profile": self.profile,
            "scheduled_total": len(self.items),
            "scheduled_by_class": self.counts(),
            "items": [item.data() for item in self.items],
        }


def _families(policy):
    if policy == "all":
        return tuple(FAMILIES)
    if policy == "rejection":
        return ("rejection",)
    if policy in FAMILIES:
        return (policy,)
    raise PlanError("unknown family policy: " + str(policy))


def _worker(profile, counter):
    return counter % profile.workers


def _generated(profile, name, counter):
    """Expand one generated class into per-program items."""
    spec = profile.klass(name)
    limit = profile.limits["max_programs"]
    if spec["programs"] > limit:
        raise PlanError(name + " schedules more programs than the profile permits")
    families = _families(spec["families"])
    live = spec["database"]
    items = []
    for index in range(spec["programs"]):
        family = families[index % len(families)]
        items.append(
            Item(
                id=name + "-" + str(index),
                run_class=name,
                ordinal=index,
                worker=_worker(profile, next(counter)) if live else None,
                seed=profile.seed,
                index=index,
                family=family,
                mode=spec["mode"],
                expected="expected-rejection" if spec["mode"] == "invalid" else "pass",
            )
        )
    return items


def _controls(profile, counter, specs):
    minimum = profile.document["controls"]["minimum_controls"]
    if len(specs) < minimum:
        raise PlanError("the control catalog is smaller than the profile requires")
    return [
        Item(
            id="control-" + spec.id,
            run_class="control",
            ordinal=ordinal,
            worker=_worker(profile, next(counter)),
            control_id=spec.id,
            expected="pass",
        )
        for ordinal, spec in enumerate(specs)
    ]


def _compile(profile):
    return [Item(id="compile-suite", run_class="compile", ordinal=0, expected="pass")]


def _counter():
    value = 0
    while True:
        yield value
        value += 1


# [spec:pgorm:req:generative.profiles]
# [spec:pgorm:req:generative.verdict]
def schedule(profile, *, controls=None):
    """Build the complete schedule, refusing one that leaves a worker idle."""
    if not isinstance(profile, profiles.Profile):
        raise PlanError("scheduling requires a validated profile")
    specs = catalog() if controls is None else controls
    counter = _counter()
    items = []
    for name in ("construction", "runtime", "invalid"):
        if profile.included(name):
            items.extend(_generated(profile, name, counter))
    if profile.included("control"):
        items.extend(_controls(profile, counter, specs))
    if profile.included("compile"):
        items.extend(_compile(profile))
    if not items:
        raise PlanError("the profile discovered no work to schedule")
    identities = [item.id for item in items]
    if len(set(identities)) != len(identities):
        raise PlanError("scheduled items must have distinct identities")
    _check_workers(profile, items)
    return Plan(profile.name, tuple(items))


def _check_workers(profile, items):
    """A declared worker that receives nothing is a worker the run never used."""
    live = [item for item in items if item.worker is not None]
    if not live:
        return
    used = {item.worker for item in live}
    missing = sorted(set(range(profile.workers)) - used)
    if missing:
        raise PlanError(
            "profile declares workers that receive no live work: "
            + ", ".join(str(worker) for worker in missing)
        )


__all__ = ["Item", "Plan", "PlanError", "schedule"]
