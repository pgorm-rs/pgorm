"""Produce installed Python execution evidence for the independent Rust oracle."""

import asyncio
import json
import os
from pathlib import Path

import pgorm as p
from pgorm import pipeline as pl, schema as s


async def runtime(db):
    table = p.Table("runtime", schema="python_entities")
    await db.execute(
        s.create_table(table)
        .column(s.ColumnDef("id", "integer").primary_key())
        .column(s.ColumnDef("name", "text"))
    )
    inserted = await db.execute(
        p.insert(table).columns("id", "name").values(1, "O'Brien 雪")
    )
    # Compiled and direct builders enter the same native execution path.
    update = (
        p.update(table)
        .set("name", "changed")
        .where_(p.col("id") == 1)
        .returning(p.col("id"), p.col("name"))
    )
    changed = await db.fetch_one(update.inspect())
    raw = await db.fetch_one(p.RawSQL("SELECT $1::text AS value", [p.Value("raw 雪")]))
    deleted = await db.execute(p.delete(table).where_(p.col("id") == 1))
    missing = await db.fetch_optional(p.select(p.col("id")).from_(table))
    await db.execute(s.drop_table(table))
    return {
        "inserted": inserted,
        "changed": [changed["id"], changed["name"]],
        "raw": raw["value"],
        "deleted": deleted,
        "missing": missing is None,
    }


def active(identity, name):
    return (
        p.entity("app.Account")
        .active()
        .set("id", identity)
        .set("display name", name)
        .set("note", None)
    )


def model(row):
    return {
        "id": row["id"],
        "name": row["display name"],
        "note": row["note"],
        "version": row["version"],
    }


def pairs(rows):
    return [
        [None if a is None else a["id"], None if n is None else n["id"]]
        for a, n in rows
    ]


async def models(db):
    await db.execute(
        p.RawSQL("TRUNCATE python_entities.notes, python_entities.accounts")
    )
    inserted = await active(1, "Native").insert(db)
    await active(2, "Other").insert(db)
    account, note = p.entity("app.Account"), p.entity("app.Note")
    await (
        note.active()
        .set("id", 11)
        .set("account_id", 1)
        .set("body", "note 雪")
        .insert(db)
    )
    query = account.find().order_by(account.col("id").expr().asc())
    all_rows = [model(row) for row in await query.all(db)]
    one = model(await query.one(db))
    missing = await query.filter(account.col("id") == 99).one_opt(db)
    graph = p.graph("app.AccountNotes").find(aliases=["n"])
    graph = graph.order_by(graph.col(0, "id").asc())
    joined = pairs(await graph.all(db))
    cursor = pairs(await graph.cursor("id").first(2).all(db))
    graph_first = (await graph.one_opt(db))[0]["id"]
    graph_missing = await graph.filter(graph.col(0, "id") == 99).one_opt(db)
    pipeline = (
        pl.from_(account)
        .join(
            pl.source(note).named("n"),
            pl.col("accounts", "id") == pl.col("n", "account_id"),
            kind=p.Join.Left,
        )
        .sort(pl.col("accounts", "id"))
    )
    selection = pl.sources("app.TwoSources")
    selected = pipeline.select_sources(selection, qualifiers=["accounts", "n"])
    sources = pairs(await selected.all(db))
    source_one = (await selected.one(db))[0]["id"]
    source_missing = (
        await pipeline.filter(False)
        .select_sources(selection, qualifiers=["accounts", "n"])
        .one_opt(db)
    )
    plain = (
        await pl.from_(account)
        .select(pl.col("accounts", "id"))
        .sort(pl.col("accounts", "id"))
        .all(db)
    )
    changed = await inserted.into_active().set("display name", "changed").update(db)
    deleted = await changed.into_active().delete(db)
    return {
        "inserted": model(inserted),
        "all": all_rows,
        "one": one,
        "missing": missing is None,
        "graph": joined,
        "cursor": cursor,
        "graph_first": graph_first,
        "graph_missing": graph_missing is None,
        "sources": sources,
        "source_one": source_one,
        "source_missing": source_missing is None,
        "pipeline": [row["id"] for row in plain],
        "updated": model(changed),
        "deleted": deleted,
    }


async def exercise(db):
    return {"runtime": await runtime(db), "models": await models(db)}


# [spec:pgorm:req:python.acceptance+1/test]
async def run():
    async with p.Pool(os.environ["PGORM_TEST_DSN"]) as pool:
        await pool.execute(p.RawSQL("CREATE SCHEMA python_entities"))
        try:
            for name in ("app.Account", "app.Note"):
                generated = s.from_entity(p.entity(name))
                for enum in generated.enums:
                    await pool.execute(enum)
                await pool.execute(generated.table)
            async with pool.connection() as db:
                connection = await exercise(db)
                transaction = await db.begin()
                transaction_report = await exercise(transaction)
                child = await transaction.begin()
                await active(3, "savepoint").insert(child)
                await child.rollback()
                account = p.entity("app.Account")
                savepoint = (
                    await account.find()
                    .filter(account.col("id") == 3)
                    .one_opt(transaction)
                )
                await transaction.commit()
                transaction = await db.begin()
                await active(4, "rolled back").insert(transaction)
                await transaction.rollback()
                rollback = (
                    await account.find().filter(account.col("id") == 4).one_opt(db)
                )
                read_only = await db.begin(mode="read_only")
                mode = await read_only.fetch_one(p.RawSQL("SHOW transaction_read_only"))
                await read_only.rollback()
                async with await db.stream(
                    p.select(p.col("id"))
                    .from_(p.Table("accounts", schema="python_entities"))
                    .order_by(p.col("id").asc())
                ) as stream:
                    streamed = [row["id"] async for row in stream]
                report = {
                    "connection": connection,
                    "transaction": transaction_report,
                    "savepoint": savepoint is None,
                    "rollback": rollback is None,
                    "read_only": mode["transaction_read_only"] == "on",
                    "stream": streamed,
                }
        finally:
            await pool.execute(p.RawSQL("DROP SCHEMA python_entities CASCADE"))
    Path(os.environ["PGORM_EXECUTION_REPORT"]).write_text(
        json.dumps(report, ensure_ascii=False, indent=2) + "\n"
    )


if __name__ == "__main__":
    asyncio.run(run())
