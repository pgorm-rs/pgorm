"""A compile-suite crate has to link the prqlc that pgorm ships.

Every batch the compile suite builds is a crate rendered at run time, with its
own `[workspace]` and a path dependency on the checkout under test. No
committed manifest describes it, so `tests/prqlc_pin_tests.rs` cannot see it.
While pgorm applied its prqlc fork through a `[patch.crates-io]` table, these
crates never repeated the table, so they resolved stock prqlc from crates.io:
every local compile suite built against a compiler pgorm does not ship, and in
CI, where only the fork had been fetched, the lock step failed outright.

The fork is now pgorm's own dependency, which a batch inherits. This renders a
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
FORK_CRATES = ("prqlc", "prqlc-parser")


def _pinned():
    """The lockfile source the root's own prqlc dependency resolves to."""
    manifest = tomllib.loads((ROOT / "Cargo.toml").read_text())
    dependency = manifest["dependencies"]["prqlc"]
    return "git+{git}?rev={rev}#{rev}".format(
        git=dependency["git"], rev=dependency["rev"]
    )


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
    async def test_a_generated_batch_links_the_fork(self):
        expected = _pinned()
        self.assertTrue(expected.startswith("git+"), expected)
        batches = _batches()
        self.assertEqual(len(batches), 2)
        for batch in batches:
            with tempfile.TemporaryDirectory() as scratch:
                emitted = compile_crate.write(batch, scratch, root=ROOT)
                manifest = Path(emitted["manifest"])
                # The rendered manifest carries no patch of its own: whatever
                # prqlc the batch links, it gets through pgorm.
                self.assertNotIn("[patch", manifest.read_text())
                await compile_runner.lock(manifest, timeout=600)
                lock = tomllib.loads((manifest.parent / "Cargo.lock").read_text())
                resolved = [
                    (package["name"], package.get("source"))
                    for package in lock["package"]
                    if package["name"] in FORK_CRATES
                ]
                self.assertIn("prqlc", [name for name, _ in resolved], batch.name)
                for name, source in resolved:
                    self.assertEqual(source, expected, f"{batch.name}: {name}")


if __name__ == "__main__":
    unittest.main()
