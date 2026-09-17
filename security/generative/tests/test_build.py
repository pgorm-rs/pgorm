from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from pgorm_campaign.build import content_identity
from pgorm_campaign.campaign_main import StaleBuild, refuse_stale


class BuildTests(unittest.TestCase):
    # [spec:pgorm:req:generative.build-amortization/test]
    def test_identity_handles_fresh_checkout_and_native_changes(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "Cargo.toml").write_text('[package]\nname="example"\n')
            source = root / "src/lib.rs"
            source.parent.mkdir()
            source.write_text("pub struct Model;\n")
            bridge = root / "security/generative/bridge"
            bridge.mkdir(parents=True)
            lock = bridge / "Cargo.lock"
            lock.write_text("version = 4\n")
            initial = content_identity(root)
            cache = root / "pgorm-example/target/debug"
            cache.mkdir(parents=True)
            (cache / "generated.rs").write_text("transient build output")
            self.assertEqual(initial, content_identity(root))
            lock.write_text("version = 4\n# changed dependency pins\n")
            self.assertNotEqual(initial, content_identity(root))
            pinned = content_identity(root)
            source.write_text("pub struct DifferentModel;\n")
            self.assertNotEqual(pinned, content_identity(root))


class StaleBuildTests(unittest.TestCase):
    """A run tests the extension that is installed, not the tree it sits in.

    On 2026-09-15 three runs reported against a subject built from earlier
    source: one called a fixed defect still broken, and a fix under test was
    neither confirmed nor refuted. The digest needed to catch that was already
    recorded; nothing on the run path read it.
    """

    @staticmethod
    def _tree(root):
        (root / "Cargo.toml").write_text('[package]\nname="example"\n')
        source = root / "src/lib.rs"
        source.parent.mkdir()
        source.write_text("pub struct Model;\n")
        bridge = root / "security/generative/bridge"
        bridge.mkdir(parents=True)
        (bridge / "Cargo.lock").write_text("version = 4\n")

    # [spec:pgorm:req:generative.build-amortization/test]
    def test_a_run_refuses_a_stale_build(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            self._tree(root)
            build = {"identity": {"source_sha256": content_identity(root)}}
            refuse_stale(build, root)

            (root / "src/lib.rs").write_text("pub struct Model;\npub struct Other;\n")
            with self.assertRaises(StaleBuild) as refusal:
                refuse_stale(build, root)
            # The refusal names both digests and what to run, since the reader
            # is looking at a campaign that declined to start.
            message = str(refusal.exception)
            self.assertIn(build["identity"]["source_sha256"], message)
            self.assertIn(content_identity(root), message)
            self.assertIn("pgorm_campaign.build", message)

    # [spec:pgorm:req:generative.build-amortization/test]
    def test_an_unidentified_build_is_refused(self):
        with TemporaryDirectory() as directory:
            root = Path(directory)
            self._tree(root)
            with self.assertRaises(StaleBuild):
                refuse_stale({}, root)
