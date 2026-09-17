# A joined, deduplicated pipeline compiles to two projection orders

**Fixed.** pgorm now pins a patched prqlc, and
`pipeline::tests::a_joined_deduplicated_relation_compiles_once` asserts the
compilation is stable. The material below is what the defect looked like.

The reproducer here no longer separates the two compilers, and this crate is
now pinned like every other detached workspace. It was left on stock prqlc so
that it would keep failing, which it did; re-measured on 2026-09-17 it does
not, on either compiler — five runs of forty compilations against stock
`0.13.14` and five against the pinned fork, one rendering every time, and that
rendering is `SELECT DISTINCT p_id, p_rank, j_id, j_account_id FROM table_1`.
pgorm now binds the deduplicated join into a CTE, so the star expansion the
fork made total is no longer on this path. The stock-prqlc failure recorded in
`native.log` is the evidence; the crate reproduces it no longer.

The same `Pipeline` value, compiled repeatedly without being rebuilt, emitted
its projection columns in one of two orders, chosen at random per compilation:

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

## The fix

`construct_tuple_from_module` in prqlc's `semantic/resolver/expr.rs` is the star
expansion. It iterates `module.names` — a `HashMap` — sorted only by each
declaration's `order`, so two declarations sharing an `order` keep whatever
relative position hash iteration gave them. Rust reseeds `RandomState` per
container, which is why the order moved within a single process rather than
only between runs. Names are unique within a module, so widening the sort key
to `(order, name)` makes it total:

```diff
-        for (name, decl) in module.names.iter().sorted_by_key(|(_, d)| d.order) {
+        for (name, decl) in module.names.iter().sorted_by_key(|(n, d)| (d.order, (*n).clone())) {
```

An earlier candidate — widening `input_cols.sort_by_key` in `lowering.rs` from
the frame position to `(position, CId)` — was built and tested against this
reproducer and did **not** fix it. It is recorded here because the plausible
site was not the real one.

The patch lives on `pgorm/deterministic-star-expansion` in
`necessary-nu/prql`, branched from the `0.13.14` tag, and the root manifest
pins prqlc to that revision. Upstream's own suite passes unchanged on the
branch (576 tests), as do pgorm's 218 lib tests and pgorm-query's 77, so the
change removes the randomness without moving any emitted SQL. The pin should be
dropped once the fix lands upstream.
