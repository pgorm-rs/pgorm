"""Evidence directories are fresh, and every document is verified as written."""

import json
from pathlib import Path
import tempfile
import unittest
import unittest.mock

from pgorm_campaign import campaign_artifacts


class FreshDirectoryTest(unittest.TestCase):
    # [spec:pgorm:req:generative.artifacts/test]
    def test_each_run_creates_its_own_directory(self):
        with tempfile.TemporaryDirectory() as root:
            first = campaign_artifacts.fresh(root, prefix="smoke")
            second = campaign_artifacts.fresh(root, prefix="smoke")
            self.assertNotEqual(first, second)
            self.assertTrue(first.is_dir() and second.is_dir())

    # [spec:pgorm:req:generative.artifacts/test]
    def test_an_existing_directory_is_never_reused(self):
        with tempfile.TemporaryDirectory() as root:
            existing = Path(root) / "run-aaaaaaaaaaaa"
            existing.mkdir()
            with unittest.mock.patch(
                "pgorm_campaign.campaign_artifacts.uuid.uuid4"
            ) as generated:
                generated.return_value.hex = "aaaaaaaaaaaa"
                with self.assertRaises(FileExistsError):
                    campaign_artifacts.fresh(root)


class ArtifactWritingTest(unittest.TestCase):
    # [spec:pgorm:req:generative.artifacts/test]
    def test_written_documents_are_hashed_and_readable(self):
        with tempfile.TemporaryDirectory() as root:
            artifacts = campaign_artifacts.Artifacts(root)
            entry = artifacts.write("nested/report.json", {"b": 1, "a": 2})
            self.assertEqual(len(entry["sha256"]), 64)
            self.assertEqual(artifacts.faults, [])
            body = (Path(root) / "nested/report.json").read_text()
            self.assertEqual(json.loads(body), {"a": 2, "b": 1})

    # [spec:pgorm:req:generative.verdict/test]
    def test_unparseable_json_is_a_malformed_artifact(self):
        with tempfile.TemporaryDirectory() as root:
            artifacts = campaign_artifacts.Artifacts(root)
            entry = artifacts.text("broken.json", "{not json", parse=True)
            self.assertIsNone(entry["sha256"])
            self.assertEqual(artifacts.faults[0]["kind"], "artifact-malformed")
            self.assertNotIn("broken.json", artifacts.entries)

    # [spec:pgorm:req:generative.verdict/test]
    def test_a_changed_artifact_is_found_at_recheck(self):
        with tempfile.TemporaryDirectory() as root:
            artifacts = campaign_artifacts.Artifacts(root)
            artifacts.write("report.json", {"a": 1})
            (Path(root) / "report.json").write_text('{"a": 2}\n')
            faults = artifacts.recheck()
            self.assertEqual(faults[0]["kind"], "artifact-malformed")

    # [spec:pgorm:req:generative.verdict/test]
    def test_a_missing_artifact_is_found_at_recheck(self):
        with tempfile.TemporaryDirectory() as root:
            artifacts = campaign_artifacts.Artifacts(root)
            artifacts.write("report.json", {"a": 1})
            (Path(root) / "report.json").unlink()
            faults = artifacts.recheck()
            self.assertEqual(faults[0]["kind"], "artifact-missing")

    # [spec:pgorm:req:generative.artifacts/test]
    def test_the_manifest_lists_what_was_written(self):
        with tempfile.TemporaryDirectory() as root:
            artifacts = campaign_artifacts.Artifacts(root)
            artifacts.write("one.json", {})
            artifacts.text("two.txt", "plain")
            manifest = artifacts.manifest()
            self.assertEqual(set(manifest["files"]), {"one.json", "two.txt"})
            self.assertEqual(manifest["directory"], root)


if __name__ == "__main__":
    unittest.main()
