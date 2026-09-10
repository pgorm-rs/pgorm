"""Minimal direct public-API check, independent of the campaign dispatcher."""

import pgorm as p
from pgorm import pipeline as pl

projected = pl.from_(p.Table("accounts", schema="fixture")).select(
    pl.col("accounts", "id")
)
print("baseline:", projected.append(projected).inspect().sql)
distinct = projected.distinct()
print("distinct:", distinct.inspect().sql)
print("distinct then append:", distinct.append(distinct).inspect().sql)
