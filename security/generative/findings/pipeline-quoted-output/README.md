# Two authored quotes become one output quote

Seed `20260911`, index `258`, from `run-3c661fded357` derives a column named
`derived24""`, nests the pipeline and projects that name. The public Python
module returns the correct values under `derived24"` (one quote). The independent
query preserves both authored quotes. Exact result metadata comparison catches
this discrepancy even though row values match.

The original program, fixture, observations, build identity and replay arguments
are retained with hashes. The verdict is an open, unattributed discrepancy;
minimization and standalone Rust replay are still required. The replay command
in `commands.json` exits 1 while the mismatch remains. No production sources
were changed. This is separate from the fixed DISTINCT regression.
