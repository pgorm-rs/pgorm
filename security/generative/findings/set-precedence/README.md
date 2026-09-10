# Chained set operations change pipeline grouping

Status: reproduced through the public Python module and the standalone Rust
renderer, whose emitted queries were executed through an independent PostgreSQL
driver. No library change or changed expectation has been applied.

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

`native.rs` constructs the same append/intersect/remove chain with public Rust
builders. `verify_native.py` builds that locked standalone crate once, provisions
an owned restricted fixture, and compares the emitted SQL's results with fully
parenthesized independent queries. `native-verification.json` retains SQL,
typed rows and source/lock/executable hashes: native counts are 8, 8 and 4;
reference counts are 8, 4 and 0. This attributes the SQL discrepancy below Python,
and does not claim general Rust execution/decoder parity.

```sh
PYTHONPATH=security/generative/src target/generative-build/venv/bin/python \
  security/generative/findings/set-precedence/verify_native.py
```

The initial diagnostic accidentally removed from the append result instead of
the preceding intersection. That setup run is retained separately under
`target/generative-native-findings/run-c637edb307e7`; the corrected chain is in
the recorded verification. A successful diagnostic command means the two known
discrepancies were detected, not that the chained program is correct.
