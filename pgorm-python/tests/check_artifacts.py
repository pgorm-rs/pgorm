"""Inspect wheel/source contents: python check_artifacts.py ARTIFACT_DIR."""

from pathlib import Path, PurePosixPath
import sys
import tarfile
import tomllib
import zipfile


# [spec:pgorm:req:python.optional/test]
# [spec:pgorm:req:python.package/test]
def check_source(path: Path) -> None:
    with tarfile.open(path) as source:
        names = source.getnames()
        root = names[0].split("/")[0]
        forbidden = {"security", "tests_cfg", ".nplan", ".codex", "plan"}
        for name in names:
            assert not forbidden.intersection(PurePosixPath(name).parts), name
            assert "this-session-is-being-continued" not in name, name
        project_file = source.extractfile(f"{root}/pyproject.toml")
        assert project_file is not None
        project = tomllib.loads(project_file.read().decode())
        native_manifest = project["tool"]["maturin"]["manifest-path"]
        native_root = PurePosixPath(native_manifest).parent
        assert f"{root}/{native_root}/src/lib.rs" in names
        lock_file = source.extractfile(f"{root}/{native_root}/Cargo.lock")
        assert lock_file is not None
        packages = tomllib.loads(lock_file.read().decode())["package"]
        assert any(p["name"] == "pyo3" and p["version"] == "0.29.2" for p in packages)


# [spec:pgorm:req:python.optional/test]
# [spec:pgorm:req:python.package/test]
def check_wheel(path: Path) -> None:
    with zipfile.ZipFile(path) as wheel:
        names = wheel.namelist()
        assert "pgorm/__init__.py" in names
        assert "pgorm/py.typed" in names
        assert any(n.startswith("pgorm/_native.") and n.endswith((".so", ".pyd")) for n in names)
        assert any(n.endswith("/LICENSE-MIT") for n in names)
        assert any(n.endswith("/LICENSE-APACHE") for n in names)
        assert not any("security/" in n or "tests_cfg/" in n or "fixtures/" in n for n in names)


if __name__ == "__main__":
    directory = Path(sys.argv[1])
    sources = sorted(directory.glob("pgorm-*.tar.gz"))
    wheels = sorted(directory.glob("pgorm-*.whl"))
    assert sources, "source distribution is missing"
    assert wheels, "wheel is missing"
    for artifact in sources:
        check_source(artifact)
    for artifact in wheels:
        check_wheel(artifact)
    print(f"Validated {len(sources)} source distribution(s) and {len(wheels)} wheel(s).")
