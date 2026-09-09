"""Run against an installed wheel, never against an in-tree extension copy."""

import importlib.machinery
import importlib.metadata
import json
import unittest

import pgorm
from pgorm import _native


class PackageTests(unittest.TestCase):
    # [spec:pgorm:req:python.package/test]
    # [spec:pgorm:def:python.api/test]
    def test_installed_native_identity(self):
        self.assertTrue(
            any(str(_native.__file__).endswith(suffix)
                for suffix in importlib.machinery.EXTENSION_SUFFIXES)
        )
        self.assertEqual(importlib.metadata.version("pgorm"), pgorm.__version__)
        manifest = pgorm.capabilities()
        self.assertEqual(manifest["package_version"], pgorm.__version__)
        self.assertEqual(manifest["pgorm_version"], pgorm.__pgorm_version__)
        self.assertEqual(manifest["transport"], "in-process")
        self.assertEqual(json.loads(json.dumps(manifest)), manifest)

    # [spec:pgorm:req:python.capabilities/test]
    def test_capability_queries_are_isolated_and_fail_closed(self):
        manifest = pgorm.capabilities()
        manifest["operations"]["invented-operation"] = {"rust_api": "fake"}
        with self.assertRaises(pgorm.UnsupportedCapabilityError):
            pgorm.require_capability("invented-operation")
        self.assertNotIn("invented-operation", pgorm.capabilities()["operations"])

    # [spec:pgorm:req:python.optional/test]
    def test_public_wheel_excludes_campaign_and_fixture_code(self):
        files = importlib.metadata.files("pgorm")
        self.assertIsNotNone(files)
        for path in files:
            self.assertNotIn("sqlmap", str(path))
            self.assertNotIn("security/", str(path))
            self.assertNotIn("fixtures/", str(path))

    # [spec:pgorm:req:python.entities/test]
    def test_standalone_registry_has_no_application_entities(self):
        self.assertEqual(pgorm.capabilities()["registrations"]["entities"], [])
        with self.assertRaises(pgorm.UnsupportedCapabilityError):
            pgorm.entity("app.Account")


if __name__ == "__main__":
    unittest.main()
