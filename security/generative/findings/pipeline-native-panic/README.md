# Native panic while compiling a generated pipeline

The original program was generated with seed `20260911`, index `83`, during
`target/generative-grammar/run-252a4f756876`. Compilation through the public
Python module raised PyO3 `PanicException` from PRQL 0.13.14 SQL generation:
`name of this column has not been to be set before generating SQL`.

The original run stopped before writing a report for this program. The exact
saved program was replayed in `run-d808694e11f2` after the executor learned to
record this native exception while preserving cancellation behavior. That
replay completed fixture cleanup and reported **incomplete**, with
`UnexpectedNativePanic` in the subject execution evidence. The original program,
replay report, fixture and unchanged native identity are retained here with
content hashes. Minimization and standalone Rust attribution are still required.

Replay with the argument list in `commands.json`; exit status 1 is expected
while this panic remains. This finding is separate from the fixed DISTINCT
regression. No production source changes were made for this finding.
