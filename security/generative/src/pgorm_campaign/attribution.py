"""Retain failures and attribute them only with comparable standalone Rust evidence."""

import hashlib
from pathlib import Path

from .comparison import InvalidOracle, encoded, exact_error, observation
from .oracles import compare


def checked(program, report):
    checks = compare(
        program.data(),
        report["subject"],
        report["reference"],
        report["subject_state"],
        report["reference_state"],
    )
    if checks != report.get("comparisons") or len(checks) != len(
        program.data()["observations"]
    ):
        raise InvalidOracle("replay oracle evidence is missing or inconsistent")
    failed = any(not item["equal"] for item in checks)
    status = (
        "defect"
        if failed
        else "expected-rejection"
        if any(item["oracle"] == "exact-error" for item in checks)
        else "pass"
    )
    if report["status"] != status:
        raise InvalidOracle("replay status does not match its independent observations")
    if any(
        report[side].get("program_sha256") != program.digest
        for side in ("subject", "reference")
    ):
        raise InvalidOracle("replay nested observations belong to another program")


# [spec:pgorm:req:generative.replay-parity]
def selected(step):
    """The pgorm APIs a run actually reached at one step.

    Agreeing rows are not enough to call two runs equivalent: an emitter that
    reached `Pipeline::filter` where the binding reached `Pipeline::filter_with`
    has drifted, and the rows can still match. Recorded paths make that visible.
    """
    paths = step.get("native_paths")
    if not isinstance(paths, list) or not all(isinstance(p, str) for p in paths):
        raise InvalidOracle("replay step has no recorded native execution evidence")
    return paths


def parity(program, first, second):
    expected = [step["id"] for step in program.data()["steps"]]
    for report in (first, second):
        if report.get("program_sha256") != program.digest or report.get(
            "status"
        ) not in ("pass", "defect", "expected-rejection"):
            raise InvalidOracle(
                "replay has no completed oracle verdict for this program"
            )
        if [step.get("id") for step in report["subject"]["steps"]] != expected:
            raise InvalidOracle("replay observations are missing or reordered")
        if report.get("cleanup_errors") or report["subject"].get("cleanup_errors"):
            raise InvalidOracle("replay cleanup is incomplete")
        checked(program, report)
    results = []
    for step, first_step, second_step in zip(
        program.data()["steps"],
        first["subject"]["steps"],
        second["subject"]["steps"],
        strict=True,
    ):
        actual = first_step.get("inspection_probe", first_step["observation"])
        expected = second_step.get("inspection_probe", second_step["observation"])
        if expected["kind"] == "error":
            cause = (
                "sqlstate:" + expected["sqlstate"]
                if expected.get("sqlstate")
                else expected["cause"]
            )
            result = exact_error(actual, {"class": expected["class"], "cause": cause})
        else:
            result = observation(
                actual, expected, ordered=step["data"].get("ordered", False)
            )
        reached, against = selected(first_step), selected(second_step)
        same = reached == against
        results.append(
            {
                "step": step["id"],
                "equal": result.equal and same,
                "reason": result.reason
                if not result.equal
                else "equal"
                if same
                else "selected API paths differ",
                "paths": {"first": reached, "second": against},
            }
        )
    if any(report.get("subject_state") is None for report in (first, second)):
        raise InvalidOracle("replay is missing final fixture state")
    equal = encoded(first["subject_state"]) == encoded(second["subject_state"])
    results.append(
        {
            "step": "final",
            "equal": equal,
            "reason": "equal" if equal else "fixture states differ",
        }
    )
    return results


def digest(value):
    return (
        isinstance(value, str)
        and len(value) == 64
        and all(c in "0123456789abcdef" for c in value)
    )


def provenance(identity, native):
    evidence = native.get("provenance", {})
    if (
        evidence.get("backend") != "standalone-rust"
        or type(evidence.get("exit_code")) is not int
        or evidence["exit_code"] != 0
        or evidence.get("timed_out")
    ):
        raise InvalidOracle(
            "standalone Rust execution evidence is absent or incomplete"
        )
    for key in ("source_sha256", "executable_sha256", "lock_sha256"):
        if not digest(evidence.get(key)):
            raise InvalidOracle("standalone Rust evidence is missing " + key)
    if (
        not digest(identity.get("source_sha256"))
        or identity["source_sha256"] != evidence["source_sha256"]
    ):
        raise InvalidOracle("Python and Rust source identities differ")


# [spec:pgorm:req:generative.attribution]
def classify(program, python, identity, native=None):
    result = {
        "classification": "unattributed",
        "comparisons": [],
        "library_change_authorized": False,
    }
    if native is None:
        return {**result, "reason": "standalone Rust replay is still required"}
    try:
        provenance(identity, native)
        checks = parity(program, python, native)
        result["comparisons"] = checks
        if python["status"] != "defect":
            return {
                **result,
                "reason": "Python did not establish an independently checked defect",
            }
        same = all(check["equal"] for check in checks)
        if native["status"] == "defect" and same:
            result.update(
                classification="native-reproduced",
                possible_causes=["native-library", "reference-oracle"],
                reason="standalone Rust reproduces the same discrepancy; reference semantics still require review",
            )
        elif native["status"] in ("pass", "expected-rejection") and not same:
            result.update(
                classification="binding-divergence",
                possible_causes=["python-binding", "replay-conversion"],
                reason="the comparable native program passes while Python observations differ",
            )
        else:
            result["reason"] = (
                "Python and Rust evidence does not isolate one failing layer"
            )
    except (InvalidOracle, KeyError, TypeError, ValueError) as error:
        result["reason"] = str(error)
    return result


# [spec:pgorm:req:generative.attribution]
def retain(directory, program, python, identity, *, native=None, commands):
    if not commands or any(
        not isinstance(argv, list)
        or not argv
        or any(not isinstance(part, str) for part in argv)
        for argv in commands.values()
    ):
        raise InvalidOracle("finding requires concrete replay argument lists")
    directory = Path(directory)
    directory.mkdir(parents=True, exist_ok=False)
    documents = {
        "program.json": program.data(),
        "fixture.json": program.data()["fixture"],
        "python.json": python,
        "identity.json": identity,
        "commands.json": commands,
        "attribution.json": classify(program, python, identity, native),
    }
    if native is not None:
        documents["native.json"] = native
    hashes = {}
    for name, value in documents.items():
        data = (encoded(value) + "\n").encode()
        (directory / name).write_bytes(data)
        hashes[name] = hashlib.sha256(data).hexdigest()
    (directory / "manifest.json").write_text(
        encoded({"version": 1, "program_sha256": program.digest, "files": hashes})
        + "\n"
    )
    return documents["attribution.json"]
