# A joined, deduplicated pipeline compiles to two projection orders

Open discrepancy. The same `Pipeline` value, compiled repeatedly without being
rebuilt, emits its projection columns in one of two orders, chosen at random per
compilation:

```
SELECT DISTINCT n.j_id, n.j_account_id, table_1.p_id, table_1.p_rank FROM ...
SELECT DISTINCT table_1.p_id, n.j_id, n.j_account_id, table_1.p_rank FROM ...
```

The CTE names are byte-identical in both. Only the order of the deduplicated
projection differs: the left relation's columns and the joined relation's
columns swap group position.

## Shape

A `join` against a nested-pipeline source, followed by `distinct`. Neither alone
is enough — `join` without the trailing `distinct`, and `distinct` without the
join, are both stable over repeated compilation. Instability appears when a
deduplicating `group` has to expand the projection of a joined relation.

## It is not the Python binding

`native.rs` reproduces it through the Rust API alone, with no Python in the
process, and exits non-zero on the assertion (`native.log`). `python.py` shows
the same two renderings through the public Python module (`python.log`).

`src/pipeline/` contains no `HashSet` or `HashMap`, so the ordering is not
pgorm's own. The projection expansion happens inside prqlc — pinned at
`=0.13.14` — and the emitted order follows its container iteration. That makes
this a defect pgorm surfaces rather than one it introduces, but the unstable SQL
is pgorm's output either way.

## Why it matters beyond text churn

The randomised list is the **output projection**, so it is the tuple layout a
client decodes positionally. In the variant with a trailing `select` the same
instability lands in `DISTINCT ON (...)`, where argument order is semantically
load-bearing: it must prefix-match `ORDER BY`, and it decides which row of each
group survives. Row-level divergence against a live database has not yet been
confirmed; the unstable compilation has.

Reproduce (both exit non-zero while the discrepancy remains):

```sh
cargo run --locked --manifest-path \
  security/generative/findings/pipeline-join-distinct-order/Cargo.toml \
  --target-dir target

PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \
  security/generative/findings/pipeline-join-distinct-order/python.py
```

Found while verifying the emitted Python reproducer against the interpreter, in
generated programs `generate(20260911, 9, family="pipeline")` and index `12`.
No production source was changed for this finding.
