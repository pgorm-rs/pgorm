"""Explicit data transformations, independent of sqlmap's scanner runtime."""

import re

from .corpus_random import entropy

VERSION = 1
TOKEN = re.compile(r"\[[A-Z][A-Z0-9_]*\]")
RULES = {
    "version": VERSION,
    "selection": "All DBMS request payloads; no risk/level filtering. Vector and response fields are inventoried, not executed or imported.",
    "unsupported": "Dynamic UNION requests need scanner-selected projection topology; unknown request fields, metadata and placeholders are unsupported.",
    "compatibility": "Boundary and request clause sets must intersect (0 is wildcard); enumerate each common where mode. Record boundary ptype and level without assuming PostgreSQL syntax validity.",
    "original": "ptype 1 uses variant+1 as numeric original; other ptypes use a seeded ASCII original. ORIGVALUE quotes and doubles apostrophes for non-digits; ORIGINAL preserves original text.",
    "placement": "where 1 prepends original and boundary prefix; where 2 prepends negative RANDNUM and prefix; where 3 replaces original and omits prefix/suffix. Join prefix to payload with one space. A request comment replaces the boundary suffix, otherwise suffix follows with one space.",
    "substitutions": "Numbered RANDNUM tokens use a seeded base plus suffix; RANDSTR tokens use a seeded ASCII string plus suffix. Stable within a context. Generic comment is '-- ', sleep is 0, delimiters are explicit seeded ASCII markers.",
    "omissions": "No URL decoding, DBMS unescaping, tamper scripts, inference probes, HTTP framing, network access or scanner execution. These are contextual input strings, not an emulation of scanner requests.",
    "role": "Every materialized payload remains a text value or identifier, never an intentional boolean expression or raw SQL operation.",
}


def numbers(text, *, maximum=9):
    if not isinstance(text, str) or not re.fullmatch(
        r"\d+(?:-\d+)?(?:,\d+(?:-\d+)?)*", text
    ):
        raise ValueError("unsupported numeric context metadata")
    result = set()
    for part in text.split(","):
        endpoints = list(map(int, part.split("-")))
        low, high = endpoints[0], endpoints[-1]
        if not 0 <= low <= high <= maximum:
            raise ValueError("context metadata outside supported range")
        result.update(range(low, high + 1))
    return sorted(result)


def context(seed, variant, ptype):
    text = entropy(seed, variant).hex()[:12]
    base = 10000 + int(text, 16) % 80000
    original = str(variant + 1) if ptype == 1 else "campaign" + text
    return {
        "seed": seed,
        "variant": variant,
        "ptype": ptype,
        "original": original,
        "number_base": base,
        "string_base": "campaign" + text,
        "substitutions": {
            "[ORIGINAL]": original,
            "[ORIGVALUE]": original
            if original.isdigit()
            else "'" + original.replace("'", "''") + "'",
            "[GENERIC_SQL_COMMENT]": "-- ",
            "[SLEEPTIME]": "0",
            "[DELIMITER_START]": "begin" + text,
            "[DELIMITER_STOP]": "end" + text,
        },
    }


def unsupported(text):
    known = set(context(0, 0, 1)["substitutions"])
    return sorted(
        {
            token
            for token in TOKEN.findall(text)
            if token not in known
            and not re.fullmatch(r"\[RAND(?:NUM|STR)\d{0,2}\]", token)
        }
    )


def replace(text, values):
    unknown = unsupported(text)
    if unknown:
        raise ValueError("unsupported contextual placeholders: " + ", ".join(unknown))

    def substitute(match):
        token = match.group()
        if token in values["substitutions"]:
            return values["substitutions"][token]
        numbered = re.fullmatch(r"\[RAND(NUM|STR)(\d*)\]", token)
        suffix = int(numbered[2] or "0")
        return (
            str(values["number_base"] + suffix)
            if numbered[1] == "NUM"
            else values["string_base"] + str(suffix)
        )

    return TOKEN.sub(substitute, text)


def clauses(template, boundary):
    left, right = set(template["clause"]), set(boundary["clause"])
    return sorted(right if 0 in left else left if 0 in right else left & right)


# [spec:pgorm:req:generative.corpus]
def instantiate(template, boundary, values, where):
    if not clauses(template, boundary) or where not in set(template["where"]) & set(
        boundary["where"]
    ):
        raise ValueError("incompatible payload and boundary context")
    payload = replace(template["payload"], values)
    comment = replace(template["comment"], values)
    if where == 3:
        return payload + comment
    original = values["original"] if where == 1 else "-" + str(values["number_base"])
    prefix = replace(boundary["prefix"], values)
    suffix = replace(boundary["suffix"], values)
    return (
        original
        + prefix
        + " "
        + payload
        + (comment or (" " + suffix if suffix else ""))
    )
