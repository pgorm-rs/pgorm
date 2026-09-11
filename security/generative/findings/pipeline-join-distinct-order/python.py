import pgorm as p
from pgorm import pipeline as pl

accounts = pl.source(p.Table("accounts", schema="fixture")).named("a")
notes = pl.source(p.Table("notes", schema="fixture")).named("n")
inner = pl.Pipeline(notes).select(
    pl.col("n", "id").as_("j_id"), pl.col("n", "account_id").as_("j_account_id")
)
query = (
    pl.Pipeline(accounts)
    .select(pl.col("a", "id").as_("p_id"), pl.col("a", "rank").as_("p_rank"))
    .join(pl.source(inner).named("n"),
          pl.alias("p_rank").eq(pl.col("n", "j_account_id")), kind=p.Join.Left)
    .distinct()
)
seen = {query.inspect().sql for _ in range(40)}
print("distinct renderings:", len(seen))
for s in sorted(seen):
    print("  ", s[s.index("SELECT DISTINCT"):][:120])
