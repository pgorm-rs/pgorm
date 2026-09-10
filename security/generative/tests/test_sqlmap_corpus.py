from pathlib import Path
import tempfile
import unittest

from pgorm_campaign import sqlmap_archive as archive, sqlmap_context as transform
from pgorm_campaign.sqlmap_inventory import inventory


def files(request, *, boundary=None):
    result = {path: b"<root/>" for path in archive.PAYLOADS}
    result[archive.PAYLOADS[0]] = (
        "<root><test><title>synthetic</title><stype>1</stype><risk>1</risk>"
        "<level>1</level><clause>1-3</clause><where>1,2,3</where>"
        "<vector>[INFERENCE]</vector><response><time>ignored</time></response>"
        "<request>" + request + "</request></test></root>"
    ).encode()
    result["data/xml/boundaries.xml"] = (
        boundary
        or (
            "<root><boundary><level>1</level><clause>1,3</clause><where>1,2,3</where>"
            "<ptype>2</ptype><prefix>')</prefix><suffix>AND [RANDNUM]=[RANDNUM]</suffix>"
            "</boundary></root>"
        )
    ).encode()
    return result


# [spec:pgorm:req:generative.corpus/test]
class SqlmapCorpusTests(unittest.TestCase):
    def test_context_substitutions_are_consistent_and_explicit(self):
        value = transform.context(123, 1, 2)
        rendered = transform.replace(
            "[RANDNUM]=[RANDNUM], [RANDNUM1], [RANDSTR], [RANDSTR1], [ORIGINAL], [ORIGVALUE], [SLEEPTIME]",
            value,
        )
        base = value["number_base"]
        text = value["string_base"]
        self.assertEqual(
            rendered,
            f"{base}={base}, {base + 1}, {text}0, {text}1, {text}, '{text}', 0",
        )
        self.assertFalse(transform.TOKEN.search(rendered))
        for text in ("[INFERENCE]", "[UNION]", "[QUERY]", "[RANDNUM100]", "[UNKNOWN]"):
            with self.assertRaisesRegex(ValueError, "unsupported contextual"):
                transform.replace(text, value)

    def test_boundaries_materialize_each_placement_mode(self):
        templates, boundaries = inventory(
            files("<payload>AND [RANDNUM]=[RANDNUM]</payload>")
        )
        template, boundary = templates[0], boundaries[0]
        values = transform.context(0, 0, 1)
        base = values["number_base"]
        suffix = f"AND {base}={base}"
        self.assertEqual(
            transform.instantiate(template, boundary, values, 1),
            f"1') {suffix} {suffix}",
        )
        self.assertEqual(
            transform.instantiate(template, boundary, values, 2),
            f"-{base}') {suffix} {suffix}",
        )
        self.assertEqual(transform.instantiate(template, boundary, values, 3), suffix)
        template["comment"] = "[GENERIC_SQL_COMMENT]"
        self.assertEqual(
            transform.instantiate(template, boundary, values, 1), f"1') {suffix}-- "
        )
        template["clause"] = [8]
        with self.assertRaisesRegex(ValueError, "incompatible"):
            transform.instantiate(template, boundary, values, 1)
        template["clause"] = [0]
        self.assertEqual(transform.clauses(template, boundary), [1, 3])

    def test_unsupported_templates_are_retained_with_reasons(self):
        for request, expected in (
            ("<payload/><char>NULL</char><columns>1-10</columns>", "dynamic scanner"),
            ("<payload>[QUERY]</payload>", "unsupported placeholders"),
            (
                "<payload>SELECT 1</payload><custom>1</custom>",
                "scanner-selected topology",
            ),
        ):
            templates, boundaries = inventory(files(request))
            self.assertEqual(len(templates), 1)
            self.assertEqual(templates[0]["status"], "unsupported")
            self.assertIn(expected, " ".join(templates[0]["reasons"]))
            self.assertEqual(
                {item["field"] for item in templates[0]["omitted"]},
                {"vector", "response"},
            )
            self.assertEqual(boundaries[0]["status"], "supported")
        bad = files("<payload>1</payload>")
        bad["data/xml/boundaries.xml"] = bad["data/xml/boundaries.xml"].replace(
            b"[RANDNUM]", b"[MISSING]"
        )
        self.assertEqual(inventory(bad)[1][0]["status"], "unsupported")

    def test_unverified_archives_and_xml_entities_are_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "sqlmap.tar.gz"
            path.write_bytes(b"untrusted archive")
            with self.assertRaisesRegex(ValueError, "SHA-256"):
                archive.read(path)
        with self.assertRaisesRegex(ValueError, "DTDs or entities"):
            archive.xml(b'<!DOCTYPE root [<!ENTITY boom "value">]><root>&boom;</root>')
        for text in ("3-1", "0-100000000", "1,x", None):
            with self.assertRaises(ValueError):
                transform.numbers(text)
