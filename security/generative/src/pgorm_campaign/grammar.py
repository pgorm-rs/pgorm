"""Versioned programs generated from typed builder productions."""

from dataclasses import asdict, dataclass
import json

from .corpus import encoded
from .grammar_pipeline import pipeline
from .grammar_models import active, cursor, entity, graph, model
from .grammar_select import select
from .grammar_sequence import rejection, sequence
from .grammar_state import State, VERSION, structure
from .grammar_types import types
from .grammar_schema import schema
from .grammar_relational import grouped, relational, sets, sources, template
from .program import Program

FAMILIES = {
    "select": select,
    "sequence": sequence,
    "pipeline": pipeline,
    "entity": entity,
    "active": active,
    "graph": graph,
    "cursor": cursor,
    "model": model,
    "types": types,
    "schema": schema,
    "grouped": grouped,
    "relational": relational,
    "sets": sets,
    "sources": sources,
    "template": template,
}


@dataclass(frozen=True)
class Generated:
    program: Program
    _recipe: bytes

    def recipe(self):
        return json.loads(self._recipe)


# [spec:pgorm:req:generative.grammar]
def generate(seed, index, *, family=None, mode="valid", limits=None, corpus=()):
    state = State(seed, index, limits=limits, corpus=corpus)
    if mode not in ("valid", "invalid"):
        raise ValueError("generation mode must be valid or invalid")
    if mode == "invalid":
        if family not in (None, "rejection"):
            raise ValueError("invalid generation requires the rejection family")
        family, rejection_case = "rejection", rejection(state)
    else:
        family = family or tuple(FAMILIES)[index % len(FAMILIES)]
        if family not in FAMILIES:
            raise ValueError("unknown valid-program family")
        FAMILIES[family](state)
        rejection_case = None
    program = state.finish()
    recipe = {
        "version": VERSION,
        "seed": seed,
        "index": index,
        "mode": mode,
        "family": family,
        "rejection_case": rejection_case,
        "limits": asdict(state.limits),
        "drawn_inputs": sorted(state.used_inputs),
        "program_sha256": program.digest,
        "structure_sha256": structure(program),
        "expected_rejections": [
            item
            for item in program.data()["observations"]
            if item["oracle"] == "exact-error"
        ],
    }
    return Generated(program, encoded(recipe))
