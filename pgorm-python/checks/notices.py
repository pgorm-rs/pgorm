"""Generate or verify the distribution's locked Rust dependency notices."""

import argparse
import hashlib
import json
from pathlib import Path
import subprocess


def digest(data):
    return hashlib.sha256(data).hexdigest()


def notice_paths(package, root):
    directory = Path(package["manifest_path"]).parent
    # Registry packages are bounded archives. Local crates share repository
    # notices; never recurse through the workspace and its build outputs.
    paths = directory.rglob("*") if package["source"] else directory.iterdir()
    result = [
        path
        for path in paths
        if path.is_file()
        and (
            path.name.lower().startswith(
                ("license", "licence", "copying", "copyright", "notice", "unlicense")
            )
            or path.name == "AUTHORS"
            or any(
                part.lower() in ("licenses", "licences")
                for part in path.relative_to(directory).parts
            )
        )
    ]
    if not result and not package["source"]:
        result = [root / "LICENSE-APACHE", root / "LICENSE-MIT"]
    return sorted(result)


def texts_for(package, root, supplements):
    texts = []
    for path in notice_paths(package, root):
        data = path.read_bytes()
        texts.append((path.name, data))
    for entry in supplements.get(f"{package['name']}@{package['version']}", []):
        data = (root / "pgorm-python/licenses" / entry["file"]).read_bytes()
        if digest(data) != entry["sha256"]:
            raise RuntimeError("supplemental license hash differs: " + entry["file"])
        texts.append((entry["source"], data))
    if package["name"] == "pg_query":
        native = Path(package["manifest_path"]).parent / "libpg_query/vendor"
        for relative in ("protobuf-c/protobuf-c.h", "xxhash/xxhash.h"):
            header = (native / relative).read_bytes()
            texts.append(
                (
                    "libpg_query/vendor/" + relative,
                    header[: header.index(b"*/") + 2] + b"\n",
                )
            )
    if not texts or not package["license"]:
        raise RuntimeError(
            f"missing license evidence for {package['name']} {package['version']}"
        )
    return texts


def generate(root):
    metadata = json.loads(
        subprocess.check_output(
            [
                "cargo",
                "metadata",
                "--manifest-path",
                str(root / "pgorm-python/Cargo.toml"),
                "--format-version",
                "1",
                "--locked",
            ],
            text=True,
        )
    )
    supplements = json.loads(
        (root / "pgorm-python/licenses/supplemental.json").read_text()
    )
    packages, notices = [], {}
    for package in sorted(
        metadata["packages"], key=lambda item: (item["name"], item["version"])
    ):
        references = []
        for source, data in texts_for(package, root, supplements):
            identity = digest(data)
            notices[identity] = data.decode("utf-8")
            references.append({"source": source, "sha256": identity})
        packages.append(
            {
                "name": package["name"],
                "version": package["version"],
                "license": package["license"],
                "repository": package["repository"],
                "source": package["source"] or "pgorm workspace",
                "notices": references,
            }
        )
    components = [
        {
            "name": "libpg_query",
            "revision": "7be1aed1f1f968a36cf541319f71e845850f0381",
            "owner": "pg_query@6.2.0",
            "license": "BSD-3-Clause",
        },
        {
            "name": "PostgreSQL parser",
            "version": "17.7",
            "owner": "pg_query@6.2.0",
            "license": "PostgreSQL",
        },
        {"name": "protobuf-c", "owner": "pg_query@6.2.0", "license": "BSD-2-Clause"},
        {"name": "xxHash", "owner": "pg_query@6.2.0", "license": "BSD-2-Clause"},
        {
            "name": "ring native cryptography (including BoringSSL and fiat code)",
            "owner": "ring@0.17.14",
            "license": "see ring notices",
        },
    ]
    report = {
        "schema_version": 1,
        "scope": "Locked Cargo graph, including build and platform-conditional dependencies; a superset of any one wheel",
        "cargo_lock_sha256": digest((root / "pgorm-python/Cargo.lock").read_bytes()),
        "python_runtime_dependencies": [],
        "packages": packages,
        "bundled_native_components": components,
    }
    text = "pgorm Python distribution — third-party notices\n\n"
    text += "The companion DEPENDENCIES.json maps packages and bundled native components to the notice hashes below.\n\n"
    for identity, body in sorted(notices.items()):
        text += f"===== SHA-256 {identity} =====\n{body}\n\n"
    return json.dumps(report, indent=2) + "\n", text


# [spec:pgorm:req:python.distribution]
def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--write",
        action="store_true",
        help="regenerate notices after dependency changes",
    )
    options = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    manifest, notices = generate(root)
    for name, content in (
        ("DEPENDENCIES.json", manifest),
        ("THIRD_PARTY_NOTICES.txt", notices),
    ):
        path = root / "pgorm-python/python/pgorm" / name
        if options.write:
            path.write_text(content)
        elif not path.is_file() or path.read_bytes() != content.encode("utf-8"):
            raise SystemExit(f"{name} is stale; run checks/notices.py --write")
    print("Verified locked dependency metadata and native component notices.")


if __name__ == "__main__":
    main()
