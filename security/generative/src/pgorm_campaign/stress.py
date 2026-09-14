"""Drive a very large number of distinct generated programs through one extension.

The claim this harness supports is deliberately narrow, and the code keeps it
narrow. A program here is generated and then constructed through the installed
native binding; no connection is opened and no oracle decides anything. The
fields that would carry those counts are present and zero rather than absent,
so the demonstration cannot be read as live coverage by omission.

Distinctness is measured on the program digest, never on the generator index.
The grammar maps some distinct indices onto byte-identical programs — about one
program in eight hundred at these limits — so an index count would overstate
the demonstration. Only 16 bytes of each digest are retained per program, which
is what makes holding a million of them at once affordable.

Nothing in this module builds anything. Construction runs inside the audit-hook
guard that already refuses subprocesses, so "zero per-program builds" is a
property the run cannot violate rather than a number the report asserts.
"""

from . import grammar
from .grammar import FAMILIES
from .resolution import Resolution
from .runtime_guard import forbid_processes

VERSION = 1

# The two instruction kinds whose construction needs a value only an executed
# effect can supply. A database-free run reaches neither, and reporting them as
# construction errors would turn a declared boundary into a fabricated defect.
LIVE_RESULT_OPS = frozenset({"result.value", "entity.result"})

PREFIX_BYTES = 16

# How many failing programs a shard keeps verbatim. A count alone is not
# evidence anyone can act on, and a million retained examples is not a report,
# so each shard keeps the first few with the index that reproduces them.
SAMPLES = 8

CLAIM = (
    "construction through the installed native binding only; no program here "
    "opened a database connection and none was decided by an oracle"
)


def _references(node):
    """Every node identity one instruction reads, list-valued inputs included."""
    for value in node["inputs"].values():
        if isinstance(value, list):
            yield from value
        else:
            yield value


# [spec:pgorm:req:generative.acceptance]
def deferred(data):
    """Node identities whose construction requires a value PostgreSQL produces."""
    blocked = {node["id"] for node in data["nodes"] if node["op"] in LIVE_RESULT_OPS}
    for node in data["nodes"]:
        if any(reference in blocked for reference in _references(node)):
            blocked.add(node["id"])
    return frozenset(blocked)


def _panic(error):
    """PyO3 raises panic as a BaseException so ordinary handlers cannot hide it."""
    return (
        type(error).__module__ == "pyo3_runtime"
        and type(error).__name__ == "PanicException"
    )


# [spec:pgorm:req:generative.acceptance]
def construct(data, p):
    """Build every root-scope instruction a database-free run can reach."""
    resolution = Resolution(data, p)
    skipped = deferred(data)
    reachable = [
        node["id"]
        for node in data["nodes"]
        if node["scope"] == "root" and node["id"] not in skipped
    ]
    outcome = {
        "nodes_constructed": 0,
        "nodes_deferred": sum(1 for node in data["nodes"] if node["id"] in skipped),
        "nodes_reachable": len(reachable),
        "error": None,
    }
    for identity in reachable:
        try:
            resolution.get(identity)
        except Exception as error:
            outcome["error"] = {"class": type(error).__name__, "cause": str(error)}
            break
        except BaseException as error:
            if not _panic(error):
                raise
            outcome["error"] = {
                "class": "UnexpectedNativePanic",
                "cause": str(error),
            }
            break
        outcome["nodes_constructed"] += 1
    return outcome


def blank():
    """A shard tally with every field the merge expects already present."""
    return {
        "version": VERSION,
        "claim": CLAIM,
        "programs_attempted": 0,
        "programs_generated": 0,
        "reached_postgresql": 0,
        "oracle_decided": 0,
        "programs_fully_constructed": 0,
        "programs_partially_constructed": 0,
        "programs_failing_construction": 0,
        "nodes_constructed": 0,
        "nodes_deferred": 0,
        "generation_errors": {},
        "construction_errors": {},
        "families": {},
        "failures": [],
        "seconds": 0.0,
    }


def _count(bucket, key):
    bucket[key] = bucket.get(key, 0) + 1


# [spec:pgorm:req:generative.acceptance]
def merge(tallies):
    """Fold shard tallies without ever producing a cross-class total."""
    total = blank()
    for tally in tallies:
        for key, value in tally.items():
            if key in ("version", "claim"):
                continue
            if isinstance(value, dict):
                for name, count in value.items():
                    total[key][name] = total[key].get(name, 0) + count
            else:
                total[key] += value
    return total


# [spec:pgorm:req:generative.acceptance]
class Digests:
    """The distinct-program accumulator, holding a prefix rather than a digest."""

    def __init__(self):
        self.prefixes = set()

    def __len__(self):
        return len(self.prefixes)

    def absorb(self, blob):
        """Merge a shard's packed prefixes and report how many were new."""
        if len(blob) % PREFIX_BYTES:
            raise ValueError("packed program digests are truncated")
        before = len(self.prefixes)
        self.prefixes.update(
            blob[at : at + PREFIX_BYTES] for at in range(0, len(blob), PREFIX_BYTES)
        )
        return len(self.prefixes) - before


def family(index):
    """The family a valid program at this index is generated from."""
    names = tuple(FAMILIES)
    return names[index % len(names)]


# [spec:pgorm:req:generative.acceptance]
def shard(seed, indices, p, *, limits, generate=grammar.generate, build=construct):
    """Generate and construct one contiguous stripe, returning tally and digests."""
    tally = blank()
    packed = bytearray()
    with forbid_processes() as attempts:
        for index in indices:
            tally["programs_attempted"] += 1
            name = family(index)
            _count(tally["families"], name)
            try:
                generated = generate(seed, index, family=name, limits=limits)
            except Exception as error:
                _count(tally["generation_errors"], type(error).__name__)
                continue
            tally["programs_generated"] += 1
            packed += bytes.fromhex(generated.program.digest)[:PREFIX_BYTES]
            outcome = build(generated.program.data(), p)
            tally["nodes_constructed"] += outcome["nodes_constructed"]
            tally["nodes_deferred"] += outcome["nodes_deferred"]
            if outcome["error"]:
                tally["programs_failing_construction"] += 1
                _count(tally["construction_errors"], outcome["error"]["class"])
                if len(tally["failures"]) < SAMPLES:
                    tally["failures"].append(
                        {
                            "index": index,
                            "family": name,
                            "program_sha256": generated.program.digest,
                            "nodes_constructed": outcome["nodes_constructed"],
                            **outcome["error"],
                        }
                    )
            elif outcome["nodes_deferred"]:
                tally["programs_partially_constructed"] += 1
            else:
                tally["programs_fully_constructed"] += 1
    return tally, bytes(packed), list(attempts)


__all__ = [
    "CLAIM",
    "Digests",
    "LIVE_RESULT_OPS",
    "PREFIX_BYTES",
    "VERSION",
    "blank",
    "construct",
    "deferred",
    "family",
    "merge",
    "shard",
]
