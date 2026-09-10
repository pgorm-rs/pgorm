# Distribution builds and evidence

`pgorm` wheels contain the Python facade, native extension, `py.typed`, stubs,
and dependency notices. The source distribution contains the Rust path
dependencies and lockfile needed to rebuild that same Python package. Neither
artifact includes the HTTP/security harness, repository plan or test fixtures.

The ABI is CPython 3.14 with the GIL. Free-threaded Python and subinterpreters
are unsupported. `support.json` records tested combinations separately from
candidate platforms. A wheel's platform tag describes its build target; the
recorded test evidence states the actual OS and Python version exercised.

## Build and verify

The distribution checker requires Rust, a C compiler, libclang, CPython 3.14
and `uv`. Maturin 1.15.0 is installed into temporary PEP 517 build environments
from the package's build requirements. The checker uses the committed Cargo
lockfile and separate Cargo target directories for standalone and source
archive builds. Cargo dependency caches may be reused.

From the repository root, with an existing TLS-enabled test database:

```sh
export PGORM_TEST_DSN="$DATABASE_URL"
export PGORM_TEST_CA="/path/to/test-ca.pem"
python pgorm-python/checks/distribution.py
```

The supplied database must allow creating and dropping the test tables and
schemas. For a disposable local cluster, use the PostgreSQL 16 binaries:

```sh
export PGORM_TEST_PG_BIN="/path/to/postgresql/bin"
python pgorm-python/tests/with_local_postgres.py \
  python pgorm-python/checks/distribution.py
```

The wrapper creates its own cluster, random port and test CA, then shuts down
and removes that cluster. The Docker-based `tests/with_postgres.py` wrapper
works with the same checker. Neither database wrapper is an application
runtime dependency.

The checker builds a wheel and source archive through PEP 517, installs the
wheel into a fresh environment without an index, and runs all standalone
tests, signature checks, strict typing and the ordinary application. It then
builds another wheel directly from the source archive and repeats those
installation checks. Tests include real native database operations, verified
TLS, hostname/CA rejection, cancellation and resource cleanup.

`target/python-distribution/` retains both wheels, the source archive, artifact
hashes, type-check evidence and the extension's dynamic library dependencies
(`otool -L` on macOS or `ldd` on Linux). `summary.json` remains failed until both
installations pass. These commands do not upload to a package registry.

## CI and supported combinations

The [Python workflow](../.github/workflows/python.yml) runs the checker for
macOS arm64 and Linux x86-64 with CPython 3.14.4. Its macOS job uses the
[`macos-15` arm64 runner](https://docs.github.com/en/actions/reference/runners/github-hosted-runners);
the Linux job uses Ubuntu 24.04. Each job checks its actual architecture,
provisions an owned PostgreSQL cluster and retains packages and test evidence
as review artifacts. Failed jobs retain their partial evidence.

Configuring a CI job does not establish a passing result. Add a combination to
`support.json`'s tested matrix only after its distribution checker has passed.
Review the exact interpreter, OS, hashes and dynamic dependencies in that run
before using or publishing its artifacts. Registry publication is a separate
action and is not part of this workflow.

The locally verified combination is macOS 26.5.1 on arm64 with CPython 3.14.4.
The configured macOS 15 and Linux jobs remain candidates until their own runs
provide passing evidence; this checkout does not claim they have run.

## Dependencies and notices

There are no Python runtime package dependencies. Rust and native components
are inventoried in `pgorm/DEPENDENCIES.json`; full notice texts are in
`pgorm/THIRD_PARTY_NOTICES.txt` and the wheel's license metadata. The inventory
includes the entire locked Cargo graph, including build dependencies and
platform-conditional packages, so it is a superset of any individual wheel.
It identifies libpg_query, its PostgreSQL parser, protobuf-c and xxHash, and
ring's bundled cryptography. System libraries and the Python interpreter are
provided by the deployment environment; the link report identifies the
extension's dynamic library dependencies.

After a dependency update, run:

```sh
python pgorm-python/checks/notices.py --write
python pgorm-python/checks/notices.py
```

The generator reads exact Cargo package metadata and their included notices.
`licenses/supplemental.json` pins source URLs and hashes for upstream notice
files omitted from some crate archives. Missing or changed evidence fails
verification. Review and update those inputs when updating the corresponding
dependencies. The generated package files and supplemental texts are committed
so installed applications do not need network access to read their notices.
