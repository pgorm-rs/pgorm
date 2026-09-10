"""Immutable input data; corpus text never becomes a program operation."""

from dataclasses import dataclass
import json

from . import wire

VERSION = 1


def encoded(value):
    return json.dumps(
        value,
        sort_keys=True,
        ensure_ascii=False,
        separators=(",", ":"),
        allow_nan=False,
    ).encode("utf-8")


# [spec:pgorm:req:generative.corpus]
@dataclass(frozen=True)
class Input:
    _encoded: bytes

    @classmethod
    def create(cls, identity, family, value, source):
        wire.validate(value)
        if not all(isinstance(v, str) and v for v in (identity, family)):
            raise ValueError("corpus inputs need an identity and family")
        roles = ["value"]
        if value["type"]["kind"] == "text" and not value["sql_null"]:
            roles.append("identifier")
        return cls(
            encoded(
                {
                    "version": VERSION,
                    "id": identity,
                    "family": family,
                    "value": value,
                    "roles": roles,
                    "source": source,
                }
            )
        )

    @classmethod
    def from_data(cls, value):
        wire.fields(value, {"version", "id", "family", "value", "roles", "source"})
        if type(value["version"]) is not int or value["version"] != VERSION:
            raise ValueError("unsupported corpus version")
        result = cls.create(
            value["id"], value["family"], value["value"], value["source"]
        )
        if result.data() != value:
            raise ValueError("unsupported corpus version or roles")
        return result

    def data(self):
        return json.loads(self._encoded)

    def value(self):
        return self.data()["value"]

    def node(self, author, *, role="value"):
        item = self.data()
        if role not in item["roles"]:
            raise ValueError("input does not support the requested data role")
        node = author.node("value", data={"value": item["value"]})
        return author.node("name", {"value": node}) if role == "identifier" else node

    def identifier_traits(self):
        item = self.data()
        if "identifier" not in item["roles"]:
            raise ValueError("only non-NULL text is an identifier input")
        text = item["value"]["data"]
        return {
            "empty": not text,
            "contains_nul": "\0" in text,
            "utf8_bytes": len(text.encode("utf-8")),
        }
