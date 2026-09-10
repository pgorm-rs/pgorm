# Chained set operations change pipeline grouping

Status: observed through the public module; standalone Rust execution comparison
remains outstanding. No library change or changed expectation has been applied.

Let `q` explicitly project the four distinct account ids from the default
fixture. The recorded program evaluates, in order:

1. `q.append(q)`: expected and observed eight rows.
2. The previous result `.intersect(q)`: expected four rows, observed eight.
3. The previous result `.remove(q)`: expected zero rows, observed four.

The independently expected counts use multiset union, intersection and
difference. The observed SQL has an unparenthesized `UNION ALL ... INTERSECT ALL`
chain. PostgreSQL binds intersection more tightly than union, which explains
the second observation; it does not preserve the pipeline's sequence of stages.

The original program, exact tagged observations and extension/source identity
are retained here. This open finding is not an accepted rejection and must remain
visible to the generated campaign, shrink/replay work and acceptance report.
Individual set-operation dispatch probes also run, but their success does not
resolve this composition failure.
