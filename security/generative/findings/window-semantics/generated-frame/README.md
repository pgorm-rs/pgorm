# Generated frame and output-name discrepancy

Seed `20260911`, index `283`, from `run-3c661fded357` authors `last(score)` with
a one-row-following frame. Its rows disagree with the independent PostgreSQL
query, consistent with the existing frame finding in the parent directory.
The output alias also contains two quotes that become one in the subject.

This original generated program therefore has two discrepancies. Keep both
while minimizing and attributing them independently; neither is an expected
passing result. Exact artifacts, hashes and replay arguments are retained here.
