"""Static rejection cases; each marker must correspond to an installed-stub diagnostic."""

import pgorm as p
from pgorm import pipeline as pl, schema as s


async def invalid(connection: p.Connection) -> None:
    await connection.execute("SELECT 1")  # expected-error: arg-type
    pending = p.merge(p.Table("t"), p.Table("s"), p.col("id") == 1)
    await connection.fetch_all(pending)  # expected-error: arg-type
    scalar: int = await connection.fetch_one(  # expected-error: assignment
        p.select(p.literal(1))
    )
    await connection.begin(mode="invented")  # expected-error: arg-type
    await connection.begin(isolation="invented")  # expected-error: arg-type
    p.Expr()  # expected-error: call-arg
    p.Compiled()  # expected-error: call-arg
    p.col("id").__eq__(other=1)  # expected-error: call-arg
    s.DataType("not_a_type")  # expected-error: arg-type
    s.create_table(p.Table("t")).column("raw SQL")  # expected-error: arg-type
    p.select(p.literal(1)).limit("3")  # expected-error: arg-type
    p.select(p.literal(1)).limit_with_ties(None)  # expected-error: arg-type
    await connection.copy_in_rows(p.CopyFrom(p.Table("t"), "text"), [[1]])  # expected-error: arg-type
    await connection.copy_out_rows(p.CopyTo(p.Table("t"), "csv"))  # expected-error: arg-type
    p.CopyFrom(p.Table("t"), "text").quote("'")  # expected-error: misc
    p.CopyFrom(p.Table("t"), "binary").header()  # expected-error: misc
    p.CopyTo(p.Table("t"), "binary").null_string("")  # expected-error: misc
    p.CopyFrom(p.Table("t"), "csv").force_quote_all()  # expected-error: attr-defined
    p.CopyTo(p.Table("t"), "csv").freeze()  # expected-error: attr-defined
    p.CopyFrom(p.Table("t"), "yaml")  # expected-error: type-var
    await connection.copy_in(p.CopyFrom(p.Table("t"), "text"), "1\tone\n")  # expected-error: arg-type
    await connection.copy_in(p.CopyTo(p.Table("t"), "text"), b"")  # expected-error: arg-type
    p.select(p.literal(1)).union(p.RawSQL("SELECT 2"))  # expected-error: arg-type
    p.select(p.literal(1)).window("w", p.col("id"))  # expected-error: arg-type
    p.ExtendedWindow("w").partition_by(p.col("id"))  # expected-error: attr-defined
    p.GroupingElement.sets()  # expected-error: call-arg
    p.Table("t").tablesample("bernoulli", 10)  # expected-error: arg-type
    p.TableFunction(p.Table("t"))  # expected-error: arg-type
    pl.from_(p.Table("t")).filter_with(42)  # expected-error: arg-type
    optional: p.Record | None = await connection.fetch_all(  # expected-error: assignment
        p.select(p.literal(1))
    )
    sqlstate: str = p.DatabaseError("client failure").sqlstate  # expected-error: assignment
