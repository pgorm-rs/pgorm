# Distinct projection followed by append fails compilation

Status: fix verified in the current worktree on 2026-09-10 through the public
Python module, standalone Rust crate and independent live PostgreSQL oracle.
The original failing evidence is retained below; it predates the fix.

Given a pipeline with the explicit projection `accounts.id`, appending it to
itself compiled, and its `distinct()` form compiled. Appending that distinct
pipeline to itself failed with “cannot combine relations with different numbers
of columns”. Both sides have the same single-column projection.

`python.py` bypasses the campaign executor. `native.rs` uses `Pipeline`, `col`,
`select`, `distinct`, `append` and `into_sql` directly, with no Python dependency
or database requirement. Their original logs record the failure. Both now pass
with the original successful-composition assertion unchanged.

`verification.json` records the rebuilt extension identity and independent live
checks: the preserved inspection program passes, and its fetch variant returns
eight rows, with ids 1, 2, 3 and 4 each appearing twice on both subject and
reference databases. Both final fixture states match and cleanup succeeds.
These named regressions run in `tests/live_oracles.py`. This verification does
not close the separate [set-precedence finding](../set-precedence/README.md).

```sh
target/generative-build/venv/bin/python security/generative/findings/distinct-append/python.py
cargo run --locked --manifest-path security/generative/findings/distinct-append/Cargo.toml
```

`minimal.program.json` isolates the same operation graph. The original
`pipeline-stages.*` artifacts also contain an unrelated invalid `take_range(0, 2)`
probe: PRQL ranges start at one. That probe error was corrected in the executor
tests; the distinct/append failure reproduced without any take stage. The original
artifact is retained unchanged and is not presented as a minimal reproducer.

`identity.json` records the original extension and native source identity. The
standalone crate has its own retained dependency lock; its source uses the same
checkout's pgorm. The current fix settles the distinct relation into a binding
before a set operation; its production changes are tracked separately from
this campaign's regression evidence.
