"""Direct public Python reproduction, without the campaign interpreter or HTTP."""

import pgorm as p
from pgorm import pipeline as pl

relation = pl.from_(p.Table("accounts", schema="fixture"))
score, identity = pl.col("accounts", "score"), pl.col("accounts", "id")
count = relation.window(pl.count(score).as_("present"), over=pl.Over()).inspect().sql
frame = (
    relation.window(
        pl.first(score).as_("head"),
        pl.last(score).as_("tail"),
        over=pl.Over().sort_by(identity).rows(1, 1),
    )
    .inspect()
    .sql
)
print("count:", count)
print("explicit frame:", frame)
assert (
    "COUNT(score)" in count
    and frame.count("ROWS BETWEEN 1 FOLLOWING AND 1 FOLLOWING") == 2
), "count's argument and first/last's explicit frame must be preserved"
