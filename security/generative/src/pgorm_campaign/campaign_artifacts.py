"""Fresh per-run evidence directories whose contents are verified as written.

A run writes into a directory it created, so it cannot inherit a previous
run's passing evidence by accident. Every document is read back and parsed
immediately after it is written: an artifact that is absent or malformed is a
fault of the run, not a detail discovered later by whoever reads the report.
"""

import hashlib
import json
from pathlib import Path
import uuid


class ArtifactError(RuntimeError):
    """Evidence could not be established for a run."""


# [spec:pgorm:req:generative.artifacts]
def fresh(root, *, prefix="run"):
    """Create a directory this run owns; refuse to reuse an existing one."""
    directory = Path(root) / (prefix + "-" + uuid.uuid4().hex[:12])
    directory.mkdir(parents=True, exist_ok=False)
    return directory


def encode(document):
    return json.dumps(document, indent=2, sort_keys=True, default=str) + "\n"


# [spec:pgorm:req:generative.artifacts]
class Artifacts:
    """A run's evidence directory, with a manifest of what it actually wrote."""

    def __init__(self, directory):
        self.directory = Path(directory)
        self.directory.mkdir(parents=True, exist_ok=True)
        self.entries = {}
        self.faults = []

    def _fault(self, kind, detail):
        self.faults.append({"kind": kind, "detail": detail})

    def write(self, name, document):
        """Write a JSON document and prove it is on disk and readable."""
        return self.text(name, encode(document), parse=True)

    def text(self, name, body, *, parse=False):
        path = self.directory / name
        path.parent.mkdir(parents=True, exist_ok=True)
        data = body.encode("utf-8")
        try:
            path.write_bytes(data)
            actual = path.read_bytes()
        except OSError as error:
            self._fault("artifact-missing", name + ": " + str(error))
            return {"name": name, "sha256": None}
        if actual != data:
            self._fault("artifact-malformed", name + ": content differs after writing")
            return {"name": name, "sha256": None}
        if parse:
            try:
                json.loads(actual)
            except ValueError as error:
                self._fault("artifact-malformed", name + ": " + str(error))
                return {"name": name, "sha256": None}
        entry = {
            "name": name,
            "sha256": hashlib.sha256(actual).hexdigest(),
            "bytes": len(actual),
        }
        self.entries[name] = entry
        return entry

    def recheck(self):
        """Re-verify every recorded artifact at the end of the run."""
        for name, entry in sorted(self.entries.items()):
            path = self.directory / name
            if not path.exists():
                self._fault("artifact-missing", name + ": disappeared during the run")
                continue
            if hashlib.sha256(path.read_bytes()).hexdigest() != entry["sha256"]:
                self._fault(
                    "artifact-malformed", name + ": content changed after writing"
                )
        return list(self.faults)

    def manifest(self):
        return {
            "directory": str(self.directory),
            "files": dict(sorted(self.entries.items())),
            "faults": list(self.faults),
        }


__all__ = ["ArtifactError", "Artifacts", "encode", "fresh"]
