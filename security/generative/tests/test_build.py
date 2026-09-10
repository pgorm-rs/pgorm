from pathlib import Path
from tempfile import TemporaryDirectory
import unittest

from pgorm_campaign.build import content_identity


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
