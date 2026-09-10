"""Exercise real disposable fixture isolation with the installed public module."""

import asyncio
import json
from pathlib import Path

import pgorm as p

from pgorm_campaign import baseline
from pgorm_campaign.fixtures import Fixture


async def observations(pool):
    result = {}
    for table in baseline.default()["tables"]:
        name = baseline.qualified(table["schema"], table["name"])
        query = p.RawSQL(
            "SELECT row_to_json(t)::text AS value FROM " + name + " AS t ORDER BY id"
        )
        result[name] = [json.loads(row["value"]) for row in await pool.fetch_all(query)]
    return result


# [spec:pgorm:req:generative.fixtures/test]
# [spec:pgorm:req:generative.isolation/test]
async def main():
    output = Path("target/generative-fixtures")
    async with Fixture(output, workers=2) as fixture:
        pair = fixture.pair()
        assert pair.subject.split(":", 2)[2].split("@", 1)[0] not in json.dumps(
            fixture.report
        )
        async with p.Pool(pair.subject) as subject, p.Pool(pair.reference) as reference:
            initial = await observations(subject)
            assert initial == await observations(reference)
            table = p.Table("accounts", schema="fixture")
            await subject.execute(
                p.update(table).set("name", "changed").where_(p.col("id") == 1)
            )
            assert initial != await observations(subject)
            assert initial == await observations(reference)
            await fixture.reset(0, baseline.default())
            assert (
                initial == await observations(subject) == await observations(reference)
            )
            async with p.Pool(fixture.pair(1).subject) as other_worker:
                assert initial == await observations(other_worker)
            for sql in (
                "CREATE ROLE escape LOGIN",
                "CREATE DATABASE escape",
                "COPY fixture.sentinels TO '/tmp/escape'",
            ):
                try:
                    await subject.execute(p.RawSQL(sql))
                except p.DatabaseError as error:
                    assert error.sqlstate == "42501", str(error)
                else:
                    raise AssertionError(
                        "restricted role allowed a privileged operation"
                    )
            try:
                await subject.fetch_one(p.RawSQL("SELECT pg_sleep(5)"))
            except p.DatabaseError as error:
                assert error.sqlstate == "57014", str(error)
            else:
                raise AssertionError("statement timeout was not enforced")
        settings = fixture.report["settings"]
        assert settings["timezone"] == "UTC" and settings["encoding"] == "UTF8"
        assert settings["collation"] == "C"
        assert settings["search_path"] == "fixture, pg_catalog"
        print(
            "Independent baselines, worker isolation, reset, role restrictions and query deadline passed"
        )
    assert fixture.report["state"] == "closed" and fixture.report["passed"]
    print("Owned container and volumes removed; fixture evidence:", output)


if __name__ == "__main__":
    asyncio.run(main())
