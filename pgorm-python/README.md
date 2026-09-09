# pgorm for Python

The `pgorm` Python package loads an optional PyO3 extension over pgorm's Rust
builders. Calls run in the Python process. PostgreSQL uses its native protocol;
there is no HTTP adapter or query dispatcher.

The package is under implementation. `pgorm.capabilities()` reports only the
operations present in the installed build. The full contract is in
`docs/spec/python.md` in the repository; unimplemented operations are not
claimed by the capability manifest.

## Build and install

From the repository root, using CPython 3.14:

```sh
uv venv target/python-dev
uv pip install --python target/python-dev/bin/python 'maturin==1.15.0'
target/python-dev/bin/maturin build --manifest-path pgorm-python/Cargo.toml \
  --interpreter target/python-dev/bin/python --out target/python-dist
uv pip install --python target/python-dev/bin/python target/python-dist/*.whl
target/python-dev/bin/python -m unittest discover -s pgorm-python/tests
```

`maturin sdist --manifest-path pgorm-python/Cargo.toml --out target/python-dist`
builds the source distribution. Build and install commands do not publish to a
registry. `support.json` records the ABI and candidate platform matrix; release
artifacts require the installation checks on each claimed platform. Registry
name availability must be checked before publication.

Python support has its own Cargo workspace, lockfile and build command. A normal
`cargo build --workspace` at the repository root does not load PyO3 or Python
configuration. CPython uses version-specific wheels; free-threaded builds and
subinterpreters are outside the supported matrix.

```python
import pgorm

print(pgorm.__version__)
print(pgorm.capabilities())
```
