# A renamed column leaves its declared position

Open generated discrepancy, found with seed `20260913`, index `34`, family
`pipeline`, in `target/generative-grammar/run-71df9efc5493`.

A `select` that renames one of its columns and is then deduplicated emits that
column **last**, not where it was declared:

```
declared:  id, p_tenant, name, score, rank
emitted:   id, name, score, rank, p_tenant
```

`p_tenant` is the only renamed column. The independent reference projects the
declared order; pgorm moves the rename to the end. Both sides return the same
row with the same values, so this is purely positional — and a result set's
column order is what a positional decode reads, so it is a real difference
rather than cosmetic text churn.

## Minimal form

`native.rs` reduces it to three columns and no database at all:

```rust
.select((col(accounts, id), col(accounts, tenant).as_("p_tenant"), col(accounts, name)))
.distinct()
```

emits `SELECT DISTINCT id, name, tenant AS p_tenant FROM fixture.accounts`.

`minimal.program.json` is the shrunk portable program — 50 nodes down to 11,
with a one-row fixture — and `shrink.json` records the predicate, budgets and
the 34 accepted reductions.

## Not the pinned prqlc patch

pgorm pins a patched prqlc for
[pipeline-join-distinct-order](../pipeline-join-distinct-order/README.md), and
that patch changes how a star expansion orders its columns, so it is the obvious
suspect. It is not the cause. `native.rs` was run against both stock
`prqlc 0.13.14` from the registry and the pinned fork, and emits the identical
SQL either way (`native.log`). The two are separate defects that happen to share
a surface: the other one is *nondeterministic* ordering of an expanded star,
this one is *deterministic* misordering of an explicit projection.

No production source was changed for this finding.

Reproduce:

```sh
cargo run --locked --manifest-path \
  security/generative/findings/pipeline-renamed-column-order/Cargo.toml \
  --target-dir target

PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \
  security/generative/tests/live_grammar.py \
  --program security/generative/findings/pipeline-renamed-column-order/program.json
```
