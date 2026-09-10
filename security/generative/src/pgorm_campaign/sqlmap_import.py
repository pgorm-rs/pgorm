"""Optional offline importer: python -m pgorm_campaign.sqlmap_import --help."""

import argparse
from hashlib import sha256
import json
from pathlib import Path

from . import sqlmap_archive as archive, sqlmap_context as transform, wire
from .corpus import Input, encoded
from .sqlmap_inventory import inventory

ARTIFACTS = ("cases.jsonl", "inventory.json")
IMPORTER = tuple(
    name + ".py"
    for name in (
        "corpus",
        "corpus_random",
        "corpus_builtin",
        "wire",
        "sqlmap_archive",
        "sqlmap_context",
        "sqlmap_inventory",
        "sqlmap_import",
    )
)


def _inputs(templates, boundaries, *, seed, variants):
    for template in templates:
        if template["status"] != "supported":
            continue
        for boundary in boundaries:
            if boundary["status"] != "supported" or not transform.clauses(
                template, boundary
            ):
                continue
            for where in sorted(set(template["where"]) & set(boundary["where"])):
                for variant in range(variants):
                    values = transform.context(seed, variant, boundary["ptype"])
                    text = transform.instantiate(template, boundary, values, where)
                    source = {
                        "kind": "sqlmap",
                        "revision": archive.REVISION,
                        "template": template["id"],
                        "boundary": boundary["id"],
                        "where": where,
                        "variant": variant,
                        "seed": seed,
                        "transform_version": transform.VERSION,
                    }
                    identity = (
                        f"sqlmap/{template['id']}/{boundary['id']}/{where}/{variant}"
                    )
                    template["cases"] += 1
                    yield Input.create(
                        identity, "sqlmap", wire.scalar("text", text), source
                    )
        if not template["cases"]:
            template["status"] = "unsupported"
            template["reasons"].append("no compatible supported boundary")


def _write_inputs(path, inputs):
    count, distinct = 0, set()
    with path.open("xb") as stream:
        for item in inputs:
            count += 1
            if count > 200000:
                raise ValueError("import exceeds its materialized input budget")
            stream.write(encoded(item.data()) + b"\n")
            distinct.add(sha256(encoded(item.value())).digest())
    return {
        "input_cases": count,
        "distinct_values": len(distinct),
        "executed_programs": 0,
    }


def _retain_sources(destination, files):
    for name, content in files.items():
        target = destination / "upstream" / name
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(content)
    for name in IMPORTER:
        target = destination / "importer" / name
        target.parent.mkdir(exist_ok=True)
        target.write_bytes(Path(__file__).with_name(name).read_bytes())


# [spec:pgorm:req:generative.corpus]
def import_archive(path, destination, *, seed=0, variants=2):
    if type(variants) is not int or not 1 <= variants <= 16:
        raise ValueError("import requires 1..16 explicit context variants")
    transform.context(seed, 0, 1)
    files = archive.read(path)
    templates, boundaries = inventory(files)
    destination = Path(destination)
    destination.mkdir(parents=True, exist_ok=False)
    summary = _write_inputs(
        destination / "cases.jsonl",
        _inputs(templates, boundaries, seed=seed, variants=variants),
    )
    summary.update(
        templates=len(templates),
        supported_templates=sum(t["status"] == "supported" for t in templates),
        unsupported_templates=sum(t["status"] == "unsupported" for t in templates),
        boundaries=len(boundaries),
        supported_boundaries=sum(b["status"] == "supported" for b in boundaries),
    )
    (destination / "inventory.json").write_bytes(
        encoded({"templates": templates, "boundaries": boundaries}) + b"\n"
    )
    _retain_sources(destination, files)
    manifest = {
        "version": 1,
        "pin": archive.PIN,
        "transformation": transform.RULES,
        "seed": seed,
        "variants": variants,
        "summary": summary,
        "contexts": [
            transform.context(seed, variant, ptype)
            for variant in range(variants)
            for ptype in sorted(
                {b["ptype"] for b in boundaries if b["status"] == "supported"}
            )
        ],
        "notices": ["upstream/" + name for name in archive.NOTICES],
        "files": {
            str(path.relative_to(destination)): sha256(path.read_bytes()).hexdigest()
            for path in sorted(destination.rglob("*"))
            if path.is_file()
        },
    }
    (destination / "manifest.json").write_bytes(encoded(manifest) + b"\n")
    return manifest


def _manifest(directory):
    manifest = json.loads((directory / "manifest.json").read_bytes())
    if (
        manifest["version"] != 1
        or manifest["pin"] != archive.PIN
        or manifest["transformation"] != transform.RULES
    ):
        raise ValueError("unsupported corpus pin or transformation")
    expected = (
        set(ARTIFACTS)
        | {"upstream/" + name for name in archive.MEMBERS}
        | {"importer/" + name for name in IMPORTER}
    )
    if set(manifest["files"]) != expected:
        raise ValueError("corpus manifest has missing or unexpected files")
    for name, expected_hash in manifest["files"].items():
        path = directory / name
        if path.is_symlink() or path.stat().st_size > 256 * 1024 * 1024:
            raise ValueError("corpus artifact exceeds its file budget or is a link")
        if sha256(path.read_bytes()).hexdigest() != expected_hash:
            raise ValueError("corpus content hash mismatch: " + name)
    return manifest


def load(directory):
    """Validate all retained evidence before returning typed data to a grammar."""
    directory = Path(directory)
    manifest = _manifest(directory)
    seen, distinct, inputs = set(), set(), []
    with (directory / "cases.jsonl").open("rb") as stream:
        while line := stream.readline(1024 * 1024 + 1):
            if len(line) > 1024 * 1024 or len(seen) >= 200000:
                raise ValueError("corpus line or case budget exceeded")
            item = Input.from_data(json.loads(line))
            identity = item.data()["id"]
            if identity in seen:
                raise ValueError("duplicate corpus identity")
            seen.add(identity)
            distinct.add(sha256(encoded(item.value())).digest())
            inputs.append(item)
    if (
        len(seen) != manifest["summary"]["input_cases"]
        or len(distinct) != manifest["summary"]["distinct_values"]
    ):
        raise ValueError("corpus counts disagree with retained content")
    return tuple(inputs)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--variants", type=int, default=2)
    arguments = parser.parse_args()
    result = import_archive(
        arguments.archive,
        arguments.output,
        seed=arguments.seed,
        variants=arguments.variants,
    )
    print(json.dumps(result["summary"], sort_keys=True))


if __name__ == "__main__":
    main()
