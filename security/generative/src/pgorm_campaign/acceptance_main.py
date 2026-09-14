"""Assemble initial acceptance and retain it.

    PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \\
      -m pgorm_campaign.acceptance_main --runtime target/generative-campaign

Each component is read from the run that produced it rather than re-derived
here, and a component that was never run stays absent rather than defaulting to
something that looks like a pass. The command exits non-zero when acceptance is
incomplete, which includes the case where it is only incomplete because a
coverage obligation is still outstanding.

`--campaign` names one runtime report directly, which is how a run kept outside
the `run/` layout `latest` walks is assembled. `--attempts` names the record of
profiles attempted, so a report cannot read as having declined a profile it
tried and lost.
"""

import argparse
import asyncio
import json
from pathlib import Path
import sys

from . import acceptance, acceptance_regressions
from .build import ROOT

CAMPAIGN = "target/generative-campaign"
COMPILE = "target/generative-compile/compile-report.json"
STRESS = "target/generative-stress/stress-report.json"
PARITY = "target/generative-parity/parity-report.json"
OUTPUT = "target/generative-acceptance"
FINDINGS = "security/generative/findings"


def _document(path):
    """Read one component's report, treating absence as absence."""
    if path is None:
        return None
    try:
        return json.loads(Path(path).read_text())
    except (OSError, ValueError):
        return None


def latest(directory):
    """The most recent campaign run under an artifacts directory, if any."""
    root = Path(directory) / "run"
    if not root.is_dir():
        return None
    runs = sorted(
        (path for path in root.iterdir() if (path / "campaign.json").is_file()),
        key=lambda path: path.stat().st_mtime,
    )
    return runs[-1] / "campaign.json" if runs else None


def directories(root):
    """The retained finding directories the registry is checked against."""
    found = Path(root) / FINDINGS
    if not found.is_dir():
        return []
    return [path.name for path in found.iterdir() if path.is_dir()]


def partial(directory):
    """What a run directory shows for a campaign that wrote no final report."""
    root = Path(directory)
    counts = {}
    for name in ("runtime", "invalid", "control"):
        found = root / name
        counts[name] = (
            len(list(found.glob("*.oracle.json")) or list(found.glob("*.json")))
            if found.is_dir()
            else 0
        )
    construction = root / "construction/programs.json"
    if construction.is_file():
        counts["construction"] = _document(construction).get(
            "construction_only_programs_constructed", 0
        )
    counts["findings"] = (
        len(list((root / "findings").iterdir())) if (root / "findings").is_dir() else 0
    )
    return counts


def _named():
    """The regression entries the registry declares, flattened for the runner."""
    return [
        {**regression, "finding": item["finding"]}
        for item in acceptance.resolved()
        for regression in item["regressions"]
    ]


# [spec:pgorm:req:generative.acceptance]
async def execute(arguments):
    """Gather every component's evidence and fold it into one document.

    Returns the document and the regression evidence behind it. Regressions are
    the one component this command can produce itself, and producing them costs
    two builds of a second pgorm in a scratch worktree. Retaining the evidence
    lets a later assembly read it back through `--regressions`, the way every
    other component is already read from the run that produced it.
    """
    root = Path(arguments.root)
    runtime = _document(arguments.campaign or latest(arguments.runtime))
    evidence = _document(arguments.regressions) or []
    if not evidence and not arguments.skip_regressions:
        evidence = await acceptance_regressions.verify(
            _named(),
            root=root,
            scratch=Path(arguments.output) / "counterfactual",
            target=root / "target",
            timeout=arguments.timeout,
            counterfactuals=not arguments.skip_counterfactuals,
        )
    attempts = _document(arguments.attempts) or []
    for attempt in attempts:
        if attempt.get("run"):
            attempt["observed"] = partial(attempt["run"])
    document = acceptance.assemble(
        runtime=runtime,
        parity=_document(arguments.parity),
        compile_document=_document(arguments.compile),
        stress=_document(arguments.stress),
        regressions=evidence,
        directories=directories(root),
        attempts=attempts,
    )
    return document, evidence


def parse(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=str(ROOT))
    parser.add_argument("--runtime", default=str(ROOT / CAMPAIGN))
    parser.add_argument("--campaign", default=None)
    parser.add_argument("--compile", default=str(ROOT / COMPILE))
    parser.add_argument("--stress", default=str(ROOT / STRESS))
    parser.add_argument("--parity", default=str(ROOT / PARITY))
    parser.add_argument("--output", default=str(ROOT / OUTPUT))
    parser.add_argument("--attempts", default=None)
    parser.add_argument("--regressions", default=None)
    parser.add_argument("--timeout", type=int, default=3600)
    parser.add_argument("--skip-regressions", action="store_true")
    parser.add_argument("--skip-counterfactuals", action="store_true")
    return parser.parse_args(argv)


def main(argv=None):
    arguments = parse(argv)
    output = Path(arguments.output)
    output.mkdir(parents=True, exist_ok=True)
    document, evidence = asyncio.run(execute(arguments))
    (output / "acceptance.json").write_text(
        json.dumps(document, indent=2, sort_keys=True) + "\n"
    )
    if evidence:
        (output / "regressions.json").write_text(
            json.dumps(evidence, indent=2, sort_keys=True) + "\n"
        )
    (output / "ACCEPTANCE.md").write_text(acceptance.render(document))
    print(acceptance.render(document), flush=True)
    return 0 if document["complete"] else 1


if __name__ == "__main__":
    sys.exit(main())
