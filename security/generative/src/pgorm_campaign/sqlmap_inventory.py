"""Account for every upstream template and boundary, including unsupported data."""

from . import sqlmap_archive as archive, sqlmap_context as transform


def metadata(element):
    return {
        "clause": transform.numbers(element.findtext("clause")),
        "where": transform.numbers(element.findtext("where"), maximum=3),
        "level": int(element.findtext("level")),
    }


def _boundary(element, index):
    result = {"id": f"boundary/{index}", "status": "supported", "reasons": []}
    try:
        result.update(metadata(element))
        result.update(
            ptype=int(element.findtext("ptype")),
            prefix=element.findtext("prefix", ""),
            suffix=element.findtext("suffix", ""),
        )
        extra = {child.tag for child in element} - {
            "level",
            "clause",
            "where",
            "ptype",
            "prefix",
            "suffix",
        }
        if extra:
            result["reasons"].append(
                "unsupported boundary fields: " + ", ".join(sorted(extra))
            )
        if not 1 <= result["ptype"] <= 8 or 0 in result["where"]:
            result["reasons"].append("unsupported parameter or placement type")
        tokens = transform.unsupported(result["prefix"] + result["suffix"])
        if tokens:
            result["reasons"].append("unsupported placeholders: " + ", ".join(tokens))
    except (ValueError, TypeError) as error:
        result["reasons"].append(str(error))
    if result["reasons"]:
        result["status"] = "unsupported"
    return result


def _template(element, path, index):
    result = {
        "id": f"{path.rsplit('/', 1)[-1]}/{index}",
        "path": path,
        "title": element.findtext("title"),
        "status": "supported",
        "reasons": [],
        "omitted": [
            {
                "field": name,
                "reason": "scanner inference or response oracle; only request data is imported",
            }
            for name in ("vector", "response")
            if element.find(name) is not None
        ],
        "cases": 0,
    }
    try:
        result.update(metadata(element))
        result.update(
            stype=int(element.findtext("stype")),
            risk=int(element.findtext("risk")),
            dbms=element.findtext("details/dbms"),
        )
        request = element.find("request")
        if request is None:
            raise ValueError("request is absent")
        result.update(
            payload=request.findtext("payload", ""),
            comment=request.findtext("comment", ""),
        )
        extra = {child.tag for child in request} - {"payload", "comment"}
        if extra:
            result["reasons"].append(
                "request fields need scanner-selected topology: "
                + ", ".join(sorted(extra))
            )
        if not result["payload"]:
            result["reasons"].append(
                "empty payload requires dynamic scanner construction"
            )
        if 0 in result["where"]:
            result["reasons"].append("unsupported placement type")
        tokens = transform.unsupported(result["payload"] + result["comment"])
        if tokens:
            result["reasons"].append("unsupported placeholders: " + ", ".join(tokens))
    except (ValueError, TypeError) as error:
        result["reasons"].append(str(error))
    if result["reasons"]:
        result["status"] = "unsupported"
    return result


# [spec:pgorm:req:generative.corpus]
def inventory(files):
    boundaries = [
        _boundary(element, index)
        for index, element in enumerate(
            archive.xml(files["data/xml/boundaries.xml"]).findall("boundary")
        )
    ]
    templates = [
        _template(element, path, index)
        for path in archive.PAYLOADS
        for index, element in enumerate(archive.xml(files[path]).findall("test"))
    ]
    return templates, boundaries
