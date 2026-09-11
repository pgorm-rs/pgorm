# Renamed hidden sort columns across nested sources

Open generated discrepancy, found with seed `20260911`, index `66`, in
`target/generative-grammar/run-a120a9b2aeca`. The original program, subject and
independent reference observations, fixture, build identity and hashes are
retained here. Native Rust attribution is still required.

The program projects `accounts.id` as `p_id`, sorts and takes a range, nests the
pipeline, removes that projection, derives a bound expression, and nests again.
The public Python module emits a CTE that reads `id AS p_id` and `name AS p_name`
from an earlier CTE exposing only `p_id` and `p_name`. PostgreSQL rejects the
subject with SQLSTATE `42703`; the independently constructed query returns rows.
See `compiled.json` for the exact emitted SQL and bound value.

This is separate from the fixed DISTINCT/append regression. No production source
was changed for this finding. The generated campaign verifier correctly fails
when this program disagrees with the reference; this is not a passing test case.

Replay the retained program through the public Python module and the independent
database oracle (expected exit status 1 while the discrepancy remains):

```sh
PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \
  security/generative/tests/live_grammar.py \
  --program security/generative/findings/pipeline-hidden-order/program.json
```

## Reduced reproducer

`minimal.program.json` is a locally reduced program that still fails the same
way, with `minimal.json` its independent check and `shrink.json` the predicate,
budgets and the 29 accepted reductions. It is 33 nodes against the original's
58, and its fixture is one table with **no rows** — the rejection happens while
PostgreSQL plans the query, so no data is needed to show it. No claim of global
minimality is made.

The frozen predicate is the subject's own account of the failure, not just the
comparison verdict: `DatabaseError` with SQLSTATE `42703`. That distinction
matters here. An earlier run reduced this program to 13 nodes whose subject
failed in PRQL compilation instead, which is a different defect that happens to
produce the same "observation categories differ" reason; the recorded witness
rejects that candidate.

What survives reduction is the shape of the bug: a `select` that renames every
column to `p_*`, a `derive`, a `sort` keyed on those new names, and two nested
sources. The emitted SQL then reads `id` from a CTE exposing only `p_id`.

```sh
PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \
  security/generative/tests/live_shrink.py \
  --program security/generative/findings/pipeline-hidden-order/program.json
```
