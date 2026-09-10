"""Read selected data from an immutable archive without loading scanner code."""

from hashlib import sha256
from io import BytesIO
from pathlib import Path
import tarfile
import xml.etree.ElementTree as ET

REVISION = "d486742eec47ba96940d35bf2dc176f60868efdd"
PIN = {
    "upstream": "https://github.com/sqlmapproject/sqlmap",
    "revision": REVISION,
    "archive": f"https://codeload.github.com/sqlmapproject/sqlmap/tar.gz/{REVISION}",
    "sha256": "a7b0b23339e81b19da94f834df2c9c352845f2e6e1752aa51bd0f5634612b45e",
}
PAYLOADS = tuple(
    "data/xml/payloads/" + name + ".xml"
    for name in (
        "boolean_blind",
        "error_based",
        "inline_query",
        "stacked_queries",
        "time_blind",
        "union_query",
    )
)
NOTICES = ("LICENSE", "doc/THIRD-PARTY.md")
MEMBERS = NOTICES + ("data/xml/boundaries.xml",) + PAYLOADS


# [spec:pgorm:req:generative.corpus]
def read(path):
    path = Path(path)
    if path.stat().st_size > 16 * 1024 * 1024:
        raise ValueError("sqlmap archive exceeds the pinned archive size budget")
    content = path.read_bytes()
    if sha256(content).hexdigest() != PIN["sha256"]:
        raise ValueError("sqlmap archive SHA-256 does not match the immutable pin")
    prefix = "sqlmap-" + REVISION + "/"
    selected = {}
    with tarfile.open(fileobj=BytesIO(content), mode="r:gz") as archive:
        for member in archive:
            relative = member.name.removeprefix(prefix)
            if relative not in MEMBERS:
                continue
            if (
                not member.isfile()
                or member.size > 2 * 1024 * 1024
                or relative in selected
            ):
                raise ValueError("invalid or duplicate pinned data member")
            selected[relative] = archive.extractfile(member).read()
    if selected.keys() != set(MEMBERS):
        raise ValueError("pinned archive is missing required data or license notices")
    return selected


def xml(content):
    if (
        len(content) > 2 * 1024 * 1024
        or b"<!DOCTYPE" in content
        or b"<!ENTITY" in content
    ):
        raise ValueError("XML must be bounded data without DTDs or entities")
    return ET.fromstring(content)
