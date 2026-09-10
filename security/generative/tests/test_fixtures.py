import json
from pathlib import Path
import tempfile
import unittest

from pgorm_campaign.fixtures import CleanupFailure, Fixture, FixtureFailure, Pair
from pgorm_campaign.process import Output


# [spec:pgorm:req:generative.isolation/test]
class FixtureTests(unittest.IsolatedAsyncioTestCase):
    async def test_external_urls_and_unbounded_workers_fail(self):
        with self.assertRaises(TypeError):
            Fixture("unused", dsn="postgres://production")
        for workers in (0, -1, 9, True):
            with self.assertRaises(ValueError):
                Fixture("unused", workers=workers)
        with self.assertRaises(FixtureFailure):
            Fixture("unused").pair()
        self.assertNotIn("secret", repr(Pair(0, "secret", "secret", ("a", "b"))))

    async def test_cleanup_rejection_is_recorded_and_command_fails(self):
        calls = []

        async def command(*args, **kwargs):
            calls.append(args)
            if "inspect" in args:
                return Output(0, json.dumps([{"Config": {"Labels": {}}}]), "")
            return Output(0, "", "")

        with tempfile.TemporaryDirectory() as directory:
            fixture = Fixture(directory, run=command)
            fixture._attempted = True
            with self.assertRaises(CleanupFailure):
                await fixture.close()
            report = json.loads((Path(directory) / "fixture.json").read_text())
            self.assertFalse(report["passed"])
            self.assertEqual(report["state"], "cleanup-failed")
            self.assertFalse(any("rm" in call for call in calls))

    async def test_start_failure_still_removes_its_container(self):
        calls = []

        async def command(*args, **kwargs):
            calls.append(args)
            if "run" in args:
                raise TimeoutError("docker startup interrupted")
            if "inspect" in args:
                return Output(
                    0,
                    json.dumps(
                        [{"Config": {"Labels": {"pgorm.generative": "disposable"}}}]
                    ),
                    "",
                )
            return Output(0, "", "")

        with tempfile.TemporaryDirectory() as directory:
            fixture = Fixture(directory, run=command)
            with self.assertRaises(TimeoutError):
                await fixture.start()
            self.assertTrue(any("rm" in call for call in calls))
            self.assertFalse(fixture.report["passed"])


if __name__ == "__main__":
    unittest.main()
