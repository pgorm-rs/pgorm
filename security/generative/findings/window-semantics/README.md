# Nullable count and explicit first/last frames

Status: reproduced through the public Python module, an independent live
PostgreSQL reference and standalone Rust. The count discrepancy is an open
native behavior/contract discrepancy; no weakened comparison has been applied
to it. The dropped-frame discrepancy is **fixed** — see the closing section.

`count(score)` is documented as `COUNT(expr)` in `src/pipeline/funcs.rs`, with
`count_rows()` separately documented as `COUNT(*)`. Both Python and Rust emit
`COUNT(*)` for the former. With fixture scores `[10, 10, 30, NULL]`, the live
window returns 4 instead of the independent `COUNT(score)` result 3.

`Over.rows(1, 1)` explicitly selects the following row. `first(score)` and
`last(score)` omit this frame in the generated SQL, while ordinary window
aggregates retain it. For id 2, the next row's score is 30: both reference values
are 30, while both subject values are 10. For the last row the frame is empty;
the subject's `first` still returns 10 instead of NULL.

The preserved `known-*.program.json` files isolate each discrepancy, and the
matching reports retain typed observations, both fixture states and comparisons.
`identity.json` records their fresh run and the unchanged extension/source
identity. `native.rs` and `python.py` bypass the campaign interpreter and require
no database. Their assertions retain the advertised count/frame expectations:
the recorded failures are findings, not passing regressions.

```sh
target/generative-build/venv/bin/python security/generative/findings/window-semantics/python.py
cargo run --locked --manifest-path security/generative/findings/window-semantics/Cargo.toml --target-dir target
```

The Rust crate's initial setup mistakes (a missing `ExprOps` import and using
`aggregate` on an ungrouped `Pipeline`) are retained in `compile-setup*.log`.
They are harness errors. `native.log` records the corrected crate compiling,
printing both discrepancies and then failing its semantic assertion.

The oracle's aggregate default frame is the whole partition; `first`/`last`
without an authored frame use PostgreSQL's default window frame. Those rules
are distinct from dropping an explicitly requested frame. Early development
runs used the wrong aggregate default and remain separate from the preserved
corrected reports. The self-check expects these known programs to report
`defect`; this verifies detection and does not make them clean campaign cases.

## The dropped frame is fixed

The frame half was a translation defect on the subject side, not a disputed
reading of `start`/`end`: the reference's `boundary()` and `Over::rows`'
documentation agree exactly. prqlc's `std.sql.prql` annotates only the
aggregates `@{window_frame=true}`, and `sql/gen_expr.rs::translate_windowed`
emits a frame clause only for an annotated call, so `first`, `last`, `lag`,
`lead`, `rank`, `rank_dense` and `row_number` were handed the authored frame
and dropped it. pgorm now writes the whole `OVER (...)` clause for those seven
itself ([spec:pgorm:sem:pipeline.window-frame]). The retained artifacts here
record the discrepancy as found; `known-first-last-frame.program.json` no
longer reproduces the frame half. The `count(score)` → `COUNT(*)` discrepancy
is untouched and remains open.
