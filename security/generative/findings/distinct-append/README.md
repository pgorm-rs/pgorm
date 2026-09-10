# Distinct projection followed by append fails compilation

Status: reproduced through the public Python module and a standalone Rust crate.
No library change or weakened expectation has been applied.

Given a pipeline with the explicit projection `accounts.id`, appending it to
itself compiles, and its `distinct()` form compiles. Appending that distinct
pipeline to itself fails with “cannot combine relations with different numbers
of columns”. Both sides have the same single-column projection.

`python.py` bypasses the campaign executor. `native.rs` uses `Pipeline`, `col`,
`select`, `distinct`, `append` and `into_sql` directly, with no Python dependency
or database requirement. Both fail as recorded in their logs. The Rust assertion
retains the expected successful composition; this is an open finding, not an
expected rejection or a passing regression.

```sh
target/generative-build/venv/bin/python security/generative/findings/distinct-append/python.py
cargo run --locked --manifest-path security/generative/findings/distinct-append/Cargo.toml
```

`minimal.program.json` isolates the same operation graph. The original
`pipeline-stages.*` artifacts also contain an unrelated invalid `take_range(0, 2)`
probe: PRQL ranges start at one. That probe error was corrected in the executor
tests; the distinct/append failure persists without any take stage. The original
artifact is retained unchanged and is not presented as a minimal reproducer.

`identity.json` records the original extension and native source identity. The
standalone crate has its own retained dependency lock; its source uses the same
checkout's pgorm. Further source attribution and any fix require separate review.
