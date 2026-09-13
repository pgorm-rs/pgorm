"""The declared profiles must be complete, versioned and internally consistent."""

import copy
import unittest

from pgorm_campaign import profiles


def variant(name="smoke"):
    document, _ = profiles.load()
    return copy.deepcopy(document["profiles"][name])


class ProfileDocumentTest(unittest.TestCase):
    # [spec:pgorm:req:generative.profiles/test]
    def test_document_declares_smoke_and_full(self):
        document, digest = profiles.load()
        self.assertEqual(document["version"], profiles.VERSION)
        self.assertEqual(set(document["profiles"]), {"smoke", "full"})
        self.assertEqual(len(digest), 64)
        self.assertEqual(tuple(document["run_classes"]), profiles.CLASSES)

    # [spec:pgorm:req:generative.profiles/test]
    def test_every_run_class_carries_a_claim(self):
        claims = profiles.claims()
        self.assertEqual(set(claims), set(profiles.CLASSES))
        self.assertIn("never", claims["construction"])

    # [spec:pgorm:req:generative.profiles/test]
    def test_smoke_declares_limits_budgets_and_controls(self):
        profile = profiles.select("smoke")
        self.assertEqual(profile.workers, 1)
        self.assertEqual(profile.seed, 20260913)
        self.assertTrue(profile.budgets["program_timeout_seconds"] > 0)
        self.assertEqual(set(profile.budgets["class_seconds"]), set(profiles.CLASSES))
        self.assertTrue(profile.document["controls"]["required"])
        self.assertEqual(profile.grammar_limits().nodes, 128)

    # [spec:pgorm:req:generative.profiles/test]
    def test_smoke_separates_every_run_class(self):
        profile = profiles.select("smoke")
        self.assertEqual(
            profile.scheduled_classes(),
            ("construction", "runtime", "invalid", "control"),
        )
        self.assertFalse(profile.included("compile"))
        self.assertTrue(profile.klass("compile")["reason"])
        self.assertFalse(profile.klass("construction")["database"])
        self.assertTrue(profile.klass("runtime")["database"])

    # [spec:pgorm:req:generative.profiles/test]
    def test_full_requires_hostile_data_and_matrix(self):
        profile = profiles.select("full")
        self.assertEqual(profile.document["coverage"]["obligations"], "full-matrix")
        self.assertEqual(profile.document["data"]["hostile"], "required")
        self.assertEqual(profile.document["data"]["ordinary"], "required")
        self.assertTrue(profile.included("compile"))
        self.assertEqual(profile.workers, 4)

    # [spec:pgorm:req:generative.profiles/test]
    def test_identity_records_the_document_hash(self):
        profile = profiles.select("smoke")
        identity = profile.identity()
        self.assertEqual(identity["document_sha256"], profile.document_sha256)
        self.assertEqual(identity["name"], "smoke")
        self.assertIn("run_classes", identity)

    def test_unknown_profile_name_is_refused(self):
        with self.assertRaises(profiles.ProfileError):
            profiles.select("nightly")


class ProfileValidationTest(unittest.TestCase):
    def assertRejected(self, document):
        with self.assertRaises(profiles.ProfileError):
            profiles.validate("candidate", document)

    # [spec:pgorm:req:generative.profiles/test]
    def test_excluded_class_needs_a_reason(self):
        document = variant()
        document["run_classes"]["compile"].pop("reason")
        self.assertRejected(document)

    # [spec:pgorm:req:generative.profiles/test]
    def test_required_controls_must_be_scheduled(self):
        document = variant()
        document["run_classes"]["control"] = {
            "included": False,
            "database": False,
            "oracle": False,
            "reason": "skipped",
        }
        self.assertRejected(document)

    # [spec:pgorm:req:generative.profiles/test]
    def test_worker_count_must_match_fixture_limit(self):
        document = variant()
        document["workers"] = 2
        self.assertRejected(document)

    # [spec:pgorm:req:generative.profiles/test]
    def test_reseeding_on_retry_is_refused(self):
        document = variant()
        document["seed_policy"]["reseed_on_retry"] = True
        self.assertRejected(document)

    # [spec:pgorm:req:generative.profiles/test]
    def test_every_class_needs_a_time_budget(self):
        document = variant()
        document["budgets"]["class_seconds"].pop("compile")
        self.assertRejected(document)

    # [spec:pgorm:req:generative.profiles/test]
    def test_non_live_class_cannot_claim_a_database(self):
        document = variant()
        document["run_classes"]["construction"]["database"] = True
        self.assertRejected(document)

    # [spec:pgorm:req:generative.profiles/test]
    def test_coverage_must_state_what_it_establishes(self):
        document = variant()
        document["coverage"]["claim"] = ""
        self.assertRejected(document)

    # [spec:pgorm:req:generative.profiles/test]
    def test_generation_limits_stay_inside_the_grammar(self):
        document = variant()
        document["generation_limits"]["nodes"] = 8
        self.assertRejected(document)


if __name__ == "__main__":
    unittest.main()
