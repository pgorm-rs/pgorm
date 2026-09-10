"""Verify optional pinned input import and retention; no database or scanner."""

import argparse
from hashlib import sha256
import json
from pathlib import Path
import tempfile

from pgorm_campaign import sqlmap_archive as archive, sqlmap_context as transform
from pgorm_campaign.corpus import encoded
from pgorm_campaign.sqlmap_import import import_archive, load


# [spec:pgorm:req:generative.corpus/test]
def verify(path, destination):
    manifest = import_archive(path, destination, seed=20260911, variants=2)
    cases = load(destination)
    inventory = json.loads((destination / "inventory.json").read_bytes())
    assert manifest["summary"]["templates"] == 365
    assert manifest["summary"]["boundaries"] == 53
    assert manifest["summary"]["supported_templates"] == 329
    assert manifest["summary"]["unsupported_templates"] == 36
    assert manifest["summary"]["input_cases"] == len(cases) > 1000
    assert manifest["summary"]["executed_programs"] == 0
    templates = {item["id"]: item for item in inventory["templates"]}
    boundaries = {item["id"]: item for item in inventory["boundaries"]}
    for item in cases:
        data, source = item.data(), item.data()["source"]
        template, boundary = (
            templates[source["template"]],
            boundaries[source["boundary"]],
        )
        context = transform.context(
            source["seed"], source["variant"], boundary["ptype"]
        )
        assert data["roles"] == ["value", "identifier"]
        assert item.value()["data"] == transform.instantiate(
            template, boundary, context, source["where"]
        )
        assert not transform.TOKEN.search(item.value()["data"])
    unsupported = [
        item for item in templates.values() if item["status"] == "unsupported"
    ]
    assert all(
        item["reasons"] and item["path"].endswith("union_query.xml")
        for item in unsupported
    )
    source = archive.read(path)
    for name in archive.MEMBERS:
        assert (destination / "upstream" / name).read_bytes() == source[name]
    with tempfile.TemporaryDirectory() as temporary:
        duplicate = Path(temporary) / "import"
        again = import_archive(path, duplicate, seed=20260911, variants=2)
        assert encoded(manifest) == encoded(again)
        (duplicate / "cases.jsonl").write_bytes(b"changed\n")
        try:
            load(duplicate)
        except ValueError as error:
            assert "hash mismatch" in str(error)
        else:
            raise AssertionError("changed imported content was accepted")
    try:
        import_archive(path, destination)
    except FileExistsError:
        pass
    else:
        raise AssertionError("original import was overwritten")
    result = {
        "passed": True,
        "summary": manifest["summary"],
        "manifest_sha256": sha256(
            (destination / "manifest.json").read_bytes()
        ).hexdigest(),
    }
    (destination / "verification.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, sort_keys=True))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    verify(args.archive, args.output)
