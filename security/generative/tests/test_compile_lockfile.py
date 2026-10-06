"""A compile-suite crate has to link the prqlc and pg_query that pgorm ships.

Every batch the compile suite builds is a crate rendered at run time, with its
own `[workspace]` and a path dependency on the checkout under test. No
committed manifest describes it, so `tests/fork_pin_tests.rs` cannot see it.
While pgorm applied its prqlc fork through a `[patch.crates-io]` table, these
crates never repeated the table, so they resolved stock prqlc from crates.io:
every local compile suite built against a compiler pgorm does not ship, and in
CI, where only the fork had been fetched, the lock step failed outright.

Both forks are now pgorm's own dependencies, which a batch inherits — prqlc
through pgorm, pg_query through pgorm and pgorm-codegen alike. This renders a
batch through the real crate writer, locks it through the runner's own lock
step, and reads the lockfile that step wrote.
"""

from pathlib import Path
import tempfile
import tomllib
import unittest

from pgorm_campaign import compile_crate, compile_runner
from pgorm_campaign.compile_case import CompileCase

ROOT = Path(__file__).resolve().parents[3]
# Each fork the root depends on, by its dependency key, with every crate its
# repository provides.
FORKS = {"prqlc": ("prqlc", "prqlc-parser"), "pg_query": ("pg_query",)}


def _pinned(dependency):
    """The lockfile source the root's own dependency on a fork resolves to."""
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    declared = manifest["dependencies"][dependency]
    return "git+{git}?rev={rev}#{rev}".format(git=declared["git"], rev=declared["rev"])


def _batches():
    """One accepted batch without extra dependencies, and one that needs serde."""
    cases = [
        CompileCase(
            id="plain",
            obligation="rust-ownership",
            verdict="accept",
            phase="typeck",
            source="    pub fn f() {}",
        ),
        CompileCase(
            id="serde",
            obligation="rust-ownership",
            verdict="accept",
            phase="typeck",
            source="    pub fn g() {}",
            needs=("serde",),
        ),
    ]
    return compile_crate.group(cases)


# [spec:pgorm:req:generative.compile-suite/test]
class GeneratedLockfileTests(unittest.IsolatedAsyncioTestCase):
    async def test_a_generated_batch_links_each_fork(self):
        expected = {dependency: _pinned(dependency) for dependency in FORKS}
        for source in expected.values():
            self.assertTrue(source.startswith("git+"), source)
        batches = _batches()
        self.assertEqual(len(batches), 2)
        for batch in batches:
            with tempfile.TemporaryDirectory() as scratch:
                emitted = compile_crate.write(batch, scratch, root=ROOT)
                manifest = Path(emitted["manifest"])
                # The rendered manifest carries no patch of its own: whatever
                # fork the batch links, it gets through pgorm.
                self.assertNotIn("[patch", manifest.read_text())
                await compile_runner.lock(manifest, timeout=600)
                lock = tomllib.loads((manifest.parent / "Cargo.lock").read_text())
                for dependency, crates in FORKS.items():
                    resolved = [
                        (package["name"], package.get("source"))
                        for package in lock["package"]
                        if package["name"] in crates
                    ]
                    self.assertIn(
                        dependency, [name for name, _ in resolved], batch.name
                    )
                    for name, source in resolved:
                        self.assertEqual(
                            source, expected[dependency], f"{batch.name}: {name}"
                        )


if __name__ == "__main__":
    unittest.main()
