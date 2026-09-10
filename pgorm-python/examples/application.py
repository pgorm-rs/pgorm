"""Run with PGORM_TEST_DSN against PostgreSQL; uses only the installed public API."""

from __future__ import annotations

import asyncio
import json
import os
from typing import assert_type
from uuid import uuid4

import pgorm as p
from pgorm import pipeline as pl, schema as s


async def run(dsn: str) -> dict[str, object]:
    suffix = uuid4().hex[:10]
    accounts = p.Model(
        "example_accounts_" + suffix,
        {
            "id": p.Column("i32", primary_key=True),
            "name": p.Column("text"),
            "visits": p.Column("i32"),
        },
    )
    events = p.Table("example_events_" + suffix)
    account_ddl = (
        s.create_table(accounts.table)
        .column(s.ColumnDef("id", "integer").primary_key())
        .column(s.ColumnDef("name", "text").not_null())
        .column(s.ColumnDef("visits", "integer").not_null().default(0))
    )
    event_ddl = (
        s.create_table(events)
        .column(s.ColumnDef("id", "integer").primary_key())
        .column(s.ColumnDef("account_id", "integer").not_null())
        .column(s.ColumnDef("kind", "text").not_null())
    )
    async with p.Pool(dsn) as pool:
        await pool.execute(account_ddl)
        try:
            await pool.execute(event_ddl)
            try:
                report = await exercise(pool, accounts, events)
            finally:
                await pool.execute(s.drop_table(events))
        finally:
            await pool.execute(s.drop_table(accounts.table))
    return report


async def exercise(
    pool: p.Pool, accounts: p.Model, events: p.Table
) -> dict[str, object]:
    for identity, name in ((1, "Nora O'Brien"), (2, "雪")):
        count = await accounts.insert(
            {"id": identity, "name": name, "visits": 0}
        ).execute(pool)
        assert_type(count, int)
        assert count == 1
    await pool.execute(
        p.insert(events).columns("id", "account_id", "kind").values(11, 1, "login")
    )
    async with pool.transaction() as transaction:
        assert_type(transaction, p.Transaction)
        changed = accounts.update({"visits": 1}).where_(accounts.key({"id": 1}))
        assert await changed.execute(transaction) == 1
        child = await transaction.begin()
        await accounts.insert({"id": 3, "name": "rolled back", "visits": 0}).execute(
            child
        )
        await child.rollback()

    a, e = accounts.table.as_("a"), events.as_("e")
    join = (
        p.select(a.col("id"), a.col("name"), e.col("kind"))
        .from_(a)
        .join(e, a.col("id") == e.col("account_id"), kind=p.Join.Left)
        .order_by(a.col("id").asc())
    )
    records = await pool.fetch_all(join)
    assert_type(records, list[p.Record])
    joined = [dict(record) for record in records]
    assert joined == [
        {"id": 1, "name": "Nora O'Brien", "kind": "login"},
        {"id": 2, "name": "雪", "kind": None},
    ]

    pipeline = (
        pl.from_(accounts)
        .filter_with(
            lambda b: pl.col(accounts.table.name, "visits") >= b.bind(p.Value(1, "i32"))
        )
        .select(pl.col(accounts.table.name, "name"))
    )
    assert_type(pipeline, pl.Pipeline)
    async with pool.connection() as connection:
        selected = await pipeline.one_opt(connection)
        assert_type(selected, p.Record | None)
        assert selected is not None and selected["name"] == "Nora O'Brien"

    streamed: list[int] = []
    async with await pool.stream(
        p.select(p.col("id")).from_(accounts.table).order_by(p.col("id").asc())
    ) as rows:
        assert_type(rows, p.ResultStream)
        async for row in rows:
            assert_type(row, p.Record)
            # Dynamic record fields are Any; schema metadata does not invent static field types.
            identity = row["id"]
            assert isinstance(identity, int)
            streamed.append(identity)
    assert streamed == [1, 2]
    removed = await accounts.delete().where_(accounts.key({"id": 2})).execute(pool)
    assert removed == 1
    missing = await accounts.find().filter(accounts.key({"id": 2})).one_opt(pool)
    assert_type(missing, p.ModelRecord | None)
    assert missing is None
    return {
        "joined": joined,
        "streamed": streamed,
        "deleted": removed,
        "transaction": "committed; child rolled back",
        "transport": p.capabilities()["transport"],
    }


if __name__ == "__main__":
    print(
        json.dumps(
            asyncio.run(run(os.environ["PGORM_TEST_DSN"])), ensure_ascii=False, indent=2
        )
    )
