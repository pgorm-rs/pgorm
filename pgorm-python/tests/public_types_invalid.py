"""Static rejection cases; each marker must correspond to an installed-stub diagnostic."""

import pgorm as p
from pgorm import pipeline as pl, schema as s


async def invalid(connection: p.Connection) -> None:
    await connection.execute("SELECT 1")  # expected-error: arg-type
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
    pl.from_(p.Table("t")).filter_with(42)  # expected-error: arg-type
    optional: p.Record | None = await connection.fetch_all(  # expected-error: assignment
        p.select(p.literal(1))
    )
    sqlstate: str = p.DatabaseError("client failure").sqlstate  # expected-error: assignment
