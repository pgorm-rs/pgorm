# Generated append/intersection discrepancy

Seed `20260911`, index `396`, from `run-3c661fded357` appends a filtered relation
to itself and then intersects it twice with the original relation. Expected row
counts are 2, 1, 1; the subject returns 2, 2, 2. This independently generated
composition detects the existing set-precedence finding in the parent directory.

The original program and independent observations are retained with hashes and
replay arguments. It remains an unattributed generated discrepancy pending the
general Rust replay path; the parent's handwritten native evidence does not
substitute for replaying this exact program. This does not reopen DISTINCT.
