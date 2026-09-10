"""Check portable value records against every installed native scalar tag."""

from datetime import date, datetime, time, timedelta, timezone
from decimal import Decimal
import hashlib
import json
from pathlib import Path
import uuid

import pgorm as p
import pgorm._native as native

from pgorm_campaign import wire


# [spec:pgorm:req:generative.format/test]
def main():
    cases = {
        "bool": True,
        "i8": -128,
        "i16": -32768,
        "i32": -(2**31),
        "i64": -(2**63),
        "u32": 2**32 - 1,
        "u64": 2**64 - 1,
        "f32": -0.0,
        "f64": float("inf"),
        "text": "O'Brien 雪; -- %_\\",
        "char": "雪",
        "bytes": b"\x00\xff'",
        "json": None,
        "decimal": Decimal("-0.00100"),
        "uuid": uuid.UUID("12345678-1234-5678-1234-567812345678"),
        "date": date(2024, 1, 2),
        "time": time(3, 4, 5, 123000),
        "datetime": datetime(2024, 1, 2, 3, 4, 5, 123456),
        "datetime_utc": datetime(2024, 1, 2, 3, 4, 5, tzinfo=timezone.utc),
        "datetime_fixed": datetime(
            2024, 1, 2, 3, 4, 5, 123000, tzinfo=timezone(timedelta(hours=2))
        ),
        "datetime_local": datetime.now().astimezone(),
        "ipnetwork": "192.0.2.129/24",
        "mac_address": b"\x00\x11\x22\x33\x44\xff",
        "vector": [1.5, -0.0],
    }
    observed = []
    inputs = list(cases.items()) + [
        (p.TypeName('State" 雪', schema="fixture"), "O'Brien 雪")
    ]
    for kind, data in inputs:
        scalar = (
            p.Value.json(data)
            if isinstance(kind, str) and kind == "json"
            else p.Value(data, kind)
        )
        values = [
            scalar,
            p.Value.null(kind),
            p.Value.array(kind, [scalar, None]),
            p.Value.array(kind, []),
            p.Value.array(kind, None),
        ]
        for value in values:
            snapshot = value.snapshot()
            assert wire.validate(json.loads(json.dumps(snapshot))) == snapshot
            observed.append(snapshot)
    expected = set(p.capabilities()["value_types"]) - {"array"}
    seen = {snapshot["type"]["kind"] for snapshot in observed} - {"array"}
    assert seen == expected
    report = {
        "passed": True,
        "scalar_kinds": sorted(seen),
        "snapshots": len(observed),
        "native_sha256": hashlib.sha256(Path(native.__file__).read_bytes()).hexdigest(),
        "values_sha256": hashlib.sha256(
            json.dumps(observed, sort_keys=True).encode()
        ).hexdigest(),
    }
    output = Path("target/generative-program")
    output.mkdir(parents=True, exist_ok=True)
    (output / "values.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
