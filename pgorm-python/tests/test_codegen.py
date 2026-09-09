"""Build-input validation and metadata typing independent of a database."""

import ast
import copy
from pathlib import Path
import tempfile
import unittest

from pgorm.codegen.config import CodegenError, rust_path, rust_string, validate
from pgorm.codegen.emit import attributes, entity_code
from pgorm.codegen.types import field_type


class CodegenTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        (self.base / "Cargo.toml").write_text('[package]\nname = "my-entities"\n')
        self.config = {
            "schema_version": 1,
            "entity_crate": ".",
            "entities": [
                {"name": "app.Entity", "rust": "thing::Entity", "python": "Thing"}
            ],
        }

    # [spec:pgorm:req:python.codegen/test]
    def test_registration_collisions_are_rejected(self):
        for update in (
            {"name": "different"},
            {"rust": "other::Entity"},
            {"name": "other", "rust": "other::Entity", "python": "ThingModel"},
        ):
            data = copy.deepcopy(self.config)
            data["entities"].append({**data["entities"][0], **update})
            with self.subTest(update=update), self.assertRaises(CodegenError):
                validate(data, self.base)
        for name in ("int", "str", "p", "ModelView", "bad.name", "_private", "class"):
            data = copy.deepcopy(self.config)
            data["entities"][0]["python"] = name
            with self.subTest(name=name), self.assertRaises(CodegenError):
                validate(data, self.base)
        data = copy.deepcopy(self.config)
        data["graphs"] = [{"name": "graph", "rust": "graph", "python": "ThingActive"}]
        with self.assertRaises(CodegenError):
            validate(data, self.base)
        data = copy.deepcopy(self.config)
        data["entities"][0]["python"] = "Key"
        data["entities"].append(
            {"name": "second", "rust": "second::Entity", "python": "Key"}
        )
        with self.assertRaises(CodegenError):
            validate(data, self.base)

    # [spec:pgorm:req:python.codegen/test]
    def test_build_input_does_not_accept_code(self):
        for path in (
            "crate::Entity",
            "r#self::Entity",
            "super::Entity",
            "a::<T>",
            "a();boom()",
            "type::Entity",
        ):
            with self.subTest(path=path), self.assertRaises(CodegenError):
                rust_path(path)
        self.assertEqual(rust_path("r#type::Entity"), "r#type::Entity")
        self.assertEqual(rust_string('a"\\\n\t\x01東京'), '"a\\"\\\\\\n\\t\\u{1}東京"')
        for key, value in (
            ("schema_version", True),
            ("module", "../x"),
            ("entities", {}),
            ("unknown", 1),
        ):
            with self.subTest(key=key), self.assertRaises(CodegenError):
                validate({**self.config, key: value}, self.base)

    # [spec:pgorm:req:python.codegen/test]
    def test_reflection_preserves_standard_value_tags(self):
        cases = [
            ("i8", "int", "i8"),
            ("i32", "int", "i32"),
            ("Option<String>", "str | None", "text"),
            ("Vec<u8>", "bytes", "bytes"),
            ("chrono::DateTime<chrono::Utc>", "datetime", "datetime_utc"),
            ("rust_decimal::Decimal", "Decimal", "decimal"),
        ]
        for rust, annotation, kind in cases:
            mapped = field_type(
                {
                    "rust_decode_type": rust,
                    "input_hint": {"kind": "scalar", "tag": "i64"},
                }
            )
            with self.subTest(rust=rust):
                self.assertEqual(
                    (mapped.annotation, mapped.kind), (annotation, repr(kind))
                )
        for rust in (None, "ApplicationAlias", "Vec<ApplicationAlias>"):
            mapped = field_type(
                {
                    "rust_decode_type": rust,
                    "input_hint": {"kind": "scalar", "tag": "i32"},
                }
            )
            self.assertEqual(mapped.annotation, "Any")
        array = field_type(
            {
                "rust_decode_type": "Option<Vec<Option<i32>>>",
                "input_hint": {
                    "kind": "array",
                    "element": {"kind": "scalar", "tag": "i32"},
                },
            }
        )
        self.assertEqual(array.annotation, "list[int | None] | None")
        self.assertIn("Value.array('i32'", array.binding())
        enum = field_type(
            {
                "rust_decode_type": "Mood",
                "input_hint": {"kind": "enum", "name": "Mood", "schema": 'a"b'},
            }
        )
        self.assertEqual(enum.annotation, "str")
        self.assertIn("schema='a\"b'", enum.binding())

    # [spec:pgorm:req:python.codegen/test]
    def test_field_aliases_resolve_real_column_names(self):
        columns = [
            {
                "name": "display name",
                "json_key": "displayName",
                "rust_decode_type": "String",
                "input_hint": {"kind": "scalar", "tag": "text"},
            }
        ]
        entry = {"name": "app.Entity", "python": "Thing", "fields": {}}
        entity = {"columns": columns}
        self.assertEqual(attributes(entity, entry)[0][1], "displayName")
        code, stubs = entity_code(
            {**entry, "fields": {"display name": "display_name"}}, entity
        )
        self.assertIn("self['display name']", "\n".join(code))
        ast.parse("\n".join(code))
        ast.parse("\n".join(stubs))
        for fields in (
            {"missing": "x"},
            {"display name": "native"},
            {"display name": "property"},
        ):
            with self.subTest(fields=fields), self.assertRaises(CodegenError):
                attributes(entity, {**entry, "fields": fields})
        with self.assertRaises(CodegenError):
            attributes(
                {"columns": columns + [{**columns[0], "name": "displayName"}]}, entry
            )
