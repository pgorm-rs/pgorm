"""Reference template scanning, independent of the native substitution lexer."""

import re
from dataclasses import dataclass

from .comparison import InvalidOracle
from .reference_sql import SQL, bound


@dataclass(frozen=True)
class Raw:
    text: str
    parameters: list

    def sql(self):
        return template(self.text, self.parameters)


def carrier(tag):
    if tag["kind"] == "array":
        return {"kind": "array", "element": carrier(tag["element"])}
    mapping = {
        "i8": "i16",
        "char": "text",
        "datetime_fixed": "datetime_utc",
        "datetime_local": "datetime_utc",
    }
    return {"kind": mapping[tag["kind"]]} if tag["kind"] in mapping else tag


async def validate_types(driver, raw):
    import uuid

    from .reference_values import quote

    identity = "oracle_" + uuid.uuid4().hex
    await driver.query(
        SQL(("PREPARE " + quote(identity) + " AS " + raw.text,)), decode=False
    )
    try:
        async with driver.connection.cursor() as cursor:
            await cursor.execute(
                "SELECT parameter_types::oid[] FROM pg_prepared_statements WHERE name = %s",
                [identity],
            )
            result = await cursor.fetchone()
        if result is None or len(result[0]) != len(raw.parameters):
            raise InvalidOracle(
                "raw SQL parameter inference and supplied values disagree"
            )
        if any(oid not in driver.codec.types for oid in result[0]):
            await driver.codec.refresh()
        for oid, value in zip(result[0], raw.parameters, strict=True):
            if driver.codec.tag(oid) != carrier(value["type"]):
                raise InvalidOracle(
                    "raw parameter conversion to the server-inferred type needs a specific rejection oracle"
                )
    finally:
        await driver.query(SQL(("DEALLOCATE " + quote(identity),)), decode=False)


def quoted_end(text, start, delimiter, *, escapes=False):
    index = start + 1
    while index < len(text):
        if escapes and text[index] == "\\":
            index += 2
        elif text[index] == delimiter:
            if text[index : index + 2] == delimiter * 2:
                index += 2
            else:
                return index + 1
        else:
            index += 1
    raise InvalidOracle("reference template has an unterminated quoted token")


def comment_end(text, start):
    depth, index = 1, start + 2
    while index < len(text):
        if text[index : index + 2] == "/*":
            depth += 1
            index += 2
        elif text[index : index + 2] == "*/":
            depth -= 1
            index += 2
            if not depth:
                return index
        else:
            index += 1
    raise InvalidOracle("reference template has an unterminated block comment")


def template(text, parameters):
    result, used = SQL(), set()
    index, start = 0, 0
    while index < len(text):
        character = text[index]
        if character in ("'", '"'):
            escape = (
                character == "'"
                and index > 0
                and text[index - 1] in "eE"
                and (
                    index < 2
                    or not (text[index - 2].isalnum() or text[index - 2] in "_$")
                )
            )
            index = quoted_end(text, index, character, escapes=escape)
        elif text[index : index + 2] == "/*":
            index = comment_end(text, index)
        elif text[index : index + 2] == "--":
            newline = re.search(r"[\r\n]", text[index + 2 :])
            index = len(text) if newline is None else index + 2 + newline.end()
        elif character == "$" and (
            index == 0 or not (text[index - 1].isalnum() or text[index - 1] in "_$")
        ):
            delimiter = re.match(r"\$(?:[^\W\d][\w]*|)\$", text[index:])
            slot = re.match(r"\$([0-9]+)", text[index:])
            if delimiter:
                token = delimiter.group()
                end = text.find(token, index + len(token))
                if end == -1:
                    raise InvalidOracle(
                        "reference template has an unterminated dollar string"
                    )
                index = end + len(token)
            elif slot:
                number = int(slot[1])
                if not 1 <= number <= len(parameters):
                    raise InvalidOracle(
                        "reference template parameter index is out of range"
                    )
                result += text[start:index]
                result += "(" + bound(parameters[number - 1]) + ")"
                used.add(number)
                index += len(slot.group())
                start = index
            else:
                index += 1
        else:
            index += 1
    if used != set(range(1, len(parameters) + 1)):
        raise InvalidOracle("reference template has unused parameter values")
    return result + text[start:]
