"""Versioned campaign profiles, declared as data and validated before scheduling.

The declaration lives in `profiles.json` rather than in Python for the same
reason `matrix.json` does: a profile is evidence. A run records the profile
document's own content hash, so a report can be checked against the profile it
claims to have run without re-executing the interpreter that read it.

Run classes are declared, counted and reported separately on purpose. A
construction-only class counts builder calls that never opened a connection; a
runtime class counts programs an independent oracle decided. Merging the two
would let a million constructor calls be read as a million checked database
programs, so no accessor here ever returns their sum.
"""

from dataclasses import dataclass
import hashlib
from importlib.resources import files
import json

from .grammar_state import Limits

VERSION = 1
DOCUMENT = "profiles.json"

# Every class a profile may schedule. Membership is closed so a typo in the
# declaration cannot quietly introduce a class nothing counts.
CLASSES = ("construction", "runtime", "invalid", "control", "compile")

# Classes whose items open a database connection and are decided by an oracle.
LIVE_CLASSES = ("runtime", "invalid", "control")

COVERAGE_OBLIGATIONS = ("scheduled-families", "full-matrix")
DATA_LEVELS = ("none", "sampled", "required")
SEED_MODES = ("pinned",)


class ProfileError(ValueError):
    """A profile declaration is malformed, incomplete or self-contradictory."""


def _require(condition, message):
    if not condition:
        raise ProfileError(message)


def _positive(value):
    return type(value) is int and value > 0


def _seconds(value):
    return type(value) in (int, float) and value >= 0


# [spec:pgorm:req:generative.profiles]
@dataclass(frozen=True)
class Profile:
    """One validated profile, addressed by name and its own declared version."""

    name: str
    document: dict
    document_sha256: str

    @property
    def version(self):
        return self.document["profile_version"]

    @property
    def workers(self):
        return self.document["workers"]

    @property
    def seed(self):
        return self.document["seed_policy"]["seed"]

    @property
    def budgets(self):
        return self.document["budgets"]

    @property
    def limits(self):
        return self.document["generation_limits"]

    def grammar_limits(self):
        """The generation budget the grammar itself accepts, from the profile."""
        return Limits(
            depth=self.limits["depth"],
            stages=self.limits["stages"],
            nodes=self.limits["nodes"],
        )

    def klass(self, name):
        return self.document["run_classes"][name]

    def included(self, name):
        return bool(self.klass(name)["included"])

    def scheduled_classes(self):
        return tuple(name for name in CLASSES if self.included(name))

    def class_seconds(self, name):
        return self.budgets["class_seconds"][name]

    def shrink_budget(self):
        return self.budgets["shrink"]

    def identity(self):
        """What the report records so a reader can pin the exact declaration."""
        return {
            "name": self.name,
            "profile_version": self.version,
            "document_version": VERSION,
            "document_sha256": self.document_sha256,
            "workers": self.workers,
            "seed_policy": self.document["seed_policy"],
            "coverage": self.document["coverage"],
            "data": self.document["data"],
            "generation_limits": self.limits,
            "fixture_limits": self.document["fixture_limits"],
            "budgets": self.budgets,
            "controls": self.document["controls"],
            "run_classes": self.document["run_classes"],
        }


def _validate_seed_policy(policy):
    _require(isinstance(policy, dict), "profile seed policy must be an object")
    _require(policy.get("mode") in SEED_MODES, "unknown profile seed mode")
    _require(
        type(policy.get("seed")) is int and 0 <= policy["seed"] < 2**64,
        "profile seed must be an unsigned 64-bit integer",
    )
    _require(
        isinstance(policy.get("derivation"), str) and policy["derivation"],
        "profile seed policy must state how per-item seeds are derived",
    )
    _require(
        policy.get("reseed_on_retry") is False,
        "a retry that reseeds would not re-run the recorded work",
    )


def _validate_coverage(coverage):
    _require(isinstance(coverage, dict), "profile coverage must be an object")
    _require(
        coverage.get("obligations") in COVERAGE_OBLIGATIONS,
        "unknown coverage obligation selector",
    )
    for key in ("operations", "shapes", "value_matrix"):
        _require(
            coverage.get(key) in ("sampled", "complete"),
            "profile must declare " + key + " coverage as sampled or complete",
        )
    _require(
        isinstance(coverage.get("claim"), str) and coverage["claim"],
        "a profile must state what its coverage does and does not establish",
    )


def _validate_data(data):
    _require(isinstance(data, dict), "profile data declaration must be an object")
    for key in ("ordinary", "hostile"):
        _require(data.get(key) in DATA_LEVELS, "unknown profile data level for " + key)
    _require(isinstance(data.get("corpus"), str), "profile must name its input corpus")
    _require(
        isinstance(data.get("external_corpus"), bool),
        "profile must say whether an external corpus import is required",
    )


def _validate_budgets(budgets):
    _require(isinstance(budgets, dict), "profile budgets must be an object")
    for key in ("program_timeout_seconds", "control_timeout_seconds"):
        _require(_seconds(budgets.get(key)) and budgets[key] > 0, "invalid " + key)
    seconds = budgets.get("class_seconds")
    _require(
        isinstance(seconds, dict) and set(seconds) == set(CLASSES),
        "profile must budget wall time for every run class",
    )
    _require(
        all(_seconds(value) for value in seconds.values()),
        "class wall-time budgets must be non-negative numbers",
    )
    _require(
        _seconds(budgets.get("total_seconds"))
        and budgets["total_seconds"] >= max(seconds.values()),
        "the total budget must cover its largest class budget",
    )
    _validate_shrink(budgets.get("shrink"))


def _validate_shrink(shrink):
    _require(isinstance(shrink, dict), "profile must declare a shrink budget")
    _require(isinstance(shrink.get("enabled"), bool), "shrink must declare enablement")
    for key in ("candidates", "passes", "offered"):
        _require(_positive(shrink.get(key)), "shrink budget needs a positive " + key)
    for key in ("seconds", "timeout"):
        _require(
            _seconds(shrink.get(key)) and shrink[key] > 0,
            "shrink budget needs a positive " + key,
        )


def _validate_controls(controls):
    _require(isinstance(controls, dict), "profile controls must be an object")
    _require(
        controls.get("required") is True, "controls are mandatory in every profile"
    )
    _require(
        type(controls.get("catalog_version")) is int,
        "profile must pin the control catalog version",
    )
    _require(
        _positive(controls.get("minimum_controls")),
        "profile must declare how many controls must run",
    )
    _require(
        controls.get("require_declared_families") is True,
        "a profile cannot waive the mandatory control comparison families",
    )


def _validate_class(name, spec):
    _require(isinstance(spec, dict), "run class " + name + " must be an object")
    _require(isinstance(spec.get("included"), bool), name + " must declare inclusion")
    for key in ("database", "oracle"):
        _require(isinstance(spec.get(key), bool), name + " must declare " + key)
    if not spec["included"]:
        _require(
            isinstance(spec.get("reason"), str) and spec["reason"],
            "an excluded run class must record why it is excluded: " + name,
        )
        return
    if name in ("construction", "runtime", "invalid"):
        _require(
            _positive(spec.get("programs")), name + " needs a positive program count"
        )
        _require(
            spec.get("mode") in ("valid", "invalid"), name + " needs a generation mode"
        )
        _require(isinstance(spec.get("families"), str), name + " needs a family policy")
    _require(
        (name in LIVE_CLASSES) == spec["database"],
        "only live classes may declare database access: " + name,
    )


def _validate_limits(limits, fixture, workers):
    _require(isinstance(limits, dict), "profile generation limits must be an object")
    try:
        Limits(depth=limits["depth"], stages=limits["stages"], nodes=limits["nodes"])
    except (KeyError, TypeError, ValueError) as error:
        raise ProfileError("generation limits the grammar refuses: " + str(error))
    _require(
        _positive(limits.get("max_programs")),
        "profile must cap how many programs it will generate",
    )
    _require(isinstance(fixture, dict), "profile fixture limits must be an object")
    _require(
        fixture.get("workers") == workers,
        "fixture worker limit and profile worker count disagree",
    )
    for key in ("reset_per_program", "rebuild_on_schema_change"):
        _require(fixture.get(key) is True, "fixture limits must declare " + key)


# [spec:pgorm:req:generative.profiles]
def validate(name, document):
    """Refuse a declaration a run could satisfy only by leaving work out."""
    _require(isinstance(document, dict), "profile " + name + " must be an object")
    _require(
        type(document.get("profile_version")) is int
        and document["profile_version"] > 0,
        "profile " + name + " must carry its own version",
    )
    _require(
        isinstance(document.get("purpose"), str) and document["purpose"],
        "profile " + name + " must state its purpose",
    )
    _validate_seed_policy(document.get("seed_policy"))
    _require(
        type(document.get("workers")) is int and 1 <= document["workers"] <= 8,
        "profile worker count must be between 1 and 8",
    )
    _validate_coverage(document.get("coverage"))
    _validate_data(document.get("data"))
    _validate_limits(
        document.get("generation_limits"),
        document.get("fixture_limits"),
        document["workers"],
    )
    _validate_budgets(document.get("budgets"))
    _validate_controls(document.get("controls"))
    classes = document.get("run_classes")
    _require(
        isinstance(classes, dict) and set(classes) == set(CLASSES),
        "a profile must decide every run class explicitly",
    )
    for key, spec in classes.items():
        _validate_class(key, spec)
    _require(
        any(spec["included"] for spec in classes.values()),
        "a profile that schedules nothing has nothing to verify",
    )
    _require(
        classes["control"]["included"],
        "a profile that requires controls must schedule them",
    )
    return document


# [spec:pgorm:req:generative.profiles]
def load():
    """Read, hash and validate the whole profile document."""
    raw = files(__package__).joinpath(DOCUMENT).read_bytes()
    digest = hashlib.sha256(raw).hexdigest()
    value = json.loads(raw)
    _require(value.get("version") == VERSION, "unsupported profile document version")
    _require(
        tuple(value.get("run_classes", ())) == CLASSES,
        "profile document run classes disagree with the runner",
    )
    claims = value.get("class_claims")
    _require(
        isinstance(claims, dict) and set(claims) == set(CLASSES),
        "every run class must carry a claim stating what its count means",
    )
    _require(
        all(isinstance(text, str) and text for text in claims.values()),
        "run class claims must be non-empty",
    )
    profiles = value.get("profiles")
    _require(
        isinstance(profiles, dict) and profiles, "profile document has no profiles"
    )
    _require(
        {"smoke", "full"} <= set(profiles),
        "the document must declare both the smoke and full profiles",
    )
    for name, document in profiles.items():
        validate(name, document)
    return value, digest


def names():
    return tuple(load()[0]["profiles"])


# [spec:pgorm:req:generative.profiles]
def select(name):
    """Return one validated profile, or refuse an unknown name."""
    value, digest = load()
    if name not in value["profiles"]:
        raise ProfileError("unknown campaign profile: " + str(name))
    return Profile(name, value["profiles"][name], digest)


def claims():
    return load()[0]["class_claims"]


__all__ = [
    "CLASSES",
    "COVERAGE_OBLIGATIONS",
    "LIVE_CLASSES",
    "Profile",
    "ProfileError",
    "VERSION",
    "claims",
    "load",
    "names",
    "select",
    "validate",
]
