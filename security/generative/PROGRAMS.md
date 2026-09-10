# Portable generated programs

Program format version 1 is JSON data. `Program(json_text)` validates the complete
artifact without importing `pgorm`, opening a database or executing code.
`Program.from_dict()` applies the same checks. A `Program` stores canonical,
immutable JSON and exposes its SHA-256 identity; `data()` returns a detached
copy. Duplicate JSON keys, nonstandard numbers, unknown fields/versions and
excessive sizes or nesting are errors.

A program contains:

- `version`, `capability_version` and an unsigned 64-bit `seed`.
- `fixture`: the complete portable baseline, including schema, column kinds,
  nullability, keys, enums and ordered rows for subject and reference execution.
- `nodes`: a topologically ordered operation graph. Each node has an `id`,
  closed-catalog `op`, binder `scope`, typed `inputs` referencing earlier nodes,
  and validated `data` options. References preserve shared subexpressions.
- `binders`: binder identities and their one consuming callback node.
- `steps`: ordered database or construction effects with the same node shape.
  Steps name the connection/transaction scope in which they run.
- `observations`: a declared oracle for every effect and the final fixture
  state. Expected errors specify their class and particular cause.

`catalog.py` maps each instruction to its input/output builder categories,
option schema, API family and Rust implementation paths. It distinguishes
runtime statement builders, Python model descriptors, concrete Rust entities,
ActiveModels, typed graphs/cursors and pipelines. The grammar must additionally
respect fixture column types, operator compatibility and nullability when it
generates executable cases.

Graph limits are 256 nodes, 128 effects and dependency depth 64. An artifact is
at most 1 MiB. Collections, values and fixtures have their own bounds. Missing,
forward and cyclic references, wrong input categories, unreachable instructions
and missing observation obligations are rejected before execution. These limits
bound individual programs; campaign profiles separately bound their counts and
runtime.

## Values and names

A value uses the native inspection layout: `version`, `type`, `sql_null` and
`data`. The campaign owns validation/deserialization; this does not add a
deserializer to the public Python API. Signed/unsigned integers use exact
decimal text, floats use IEEE hexadecimal bits, bytes use integer byte arrays,
and decimals preserve coefficient, sign and scale. Temporal data retains native
Chrono text or exact ISO text; precision that Python cannot retain is rejected.
Enum tags carry the schema and type name; array elements retain their full
tags, including SQL NULL and JSON null as distinct values.

Names remain identifier data in their declared contexts, including quotes,
Unicode and schema identity. They are not converted to SQL operators. Raw
templates are an explicit, separate instruction whose parameter inputs are
still tagged values. Rust replay must emit the named builder operations and
encode both names and values safely as source data.

`result.value` identifies an earlier row-producing step, row index, column
identity and expected value tag. A consuming effect cannot precede that result.
The executor checks the actual returned field and type before reusing it.
The `name` instruction converts a text value into identifier data, including
values read by an earlier step. Tables, columns and projection aliases accept
either a literal name or a name-node reference, so stored identifiers remain
distinct from SQL structure throughout the graph.

## Scopes

Ordinary owned builders have scope `root`. Pipeline bound expressions name a
binder scope and can only be consumed within that scope or by the declared
owning `*_with` callback. Passing a bound expression into another pipeline,
returning it as a root value, or using it in a callback's unbound source/over
configuration is a format error.

Transaction effects maintain a checked stack. `begin` reserves its parent;
effects run only in the active child until `commit` or `rollback` closes it.
Nested scopes require unique identities and depth at most eight. Every normal
program closes its transactions. Transaction streaming is rejected because the
public binding provides streams on pools and connections.

## Coverage obligations

`src/pgorm_campaign/matrix.json` defines required operation families, structural
variations, hostile contexts and compiled registrations. `matrix.obligations()`
combines these with every catalog instruction and effect. Planned or scheduled
labels do not satisfy coverage: execution must record the actual public/native
path and a checked observation.

The matrix records compile-only and unsupported paths with reasons. For
example, nested pipeline sources provide runtime query nesting, while the
public Python `Select` does not expose `from_subquery`. Fresh entity derives,
generic graph shapes and Rust ownership rejection belong to the bounded compile
suite. Native value construction and explicit wire/decode rejection are tracked
separately for values without a supported PostgreSQL representation.

The format and matrix define the contract for generation, execution, shrinking
and replay. Passing their validation tests alone does not establish a completed
campaign.

Run the installed native value-format check from the repository root:

```sh
PYTHONPATH=security/generative/src target/python-check/bin/python \
  security/generative/tests/live_wire.py
```

It validates snapshots for every advertised scalar/enum kind, plus typed NULL,
nullable-element, empty and NULL arrays. The report records the native extension
hash and inventory in `target/generative-program/values.json`.
