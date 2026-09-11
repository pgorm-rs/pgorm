# Standalone replay

A finding keeps more than a JSON program: it keeps something a person can run.
This package emits a **Python reproducer** and **Rust source** for a portable
program, and checks that both reach the same observations through the same named
pgorm APIs.

## What is emitted

For a program, `replay.emit(program, directory)` writes:

| File | What it is |
|---|---|
| `program.json` | the portable program, unchanged |
| `fixture.json` | its declared baseline, extracted for the reader |
| `python.py` | a runnable reproducer over the public Python module |
| `rust/Cargo.toml` | a detached crate depending on `pgorm` by path |
| `rust/src/main.rs` | generated construction and effects |
| `rust/src/entities.rs` | the compiled entity, model and graph declarations |
| `manifest.json` | sha256 of every emitted file plus the program identity |

The Rust crate carries its own `[workspace]` table so it resolves against its
own lock rather than pgorm's, and depends on the checkout under test by relative
path. It needs neither Python nor sqlmap to build or run.

## The division of labour

The Rust binary is the **subject**, and only the subject. It connects to one
database, applies the declared fixture, constructs the program's builders, runs
its effects, and prints one executor-shaped report on stdout:

```json
{"program_sha256": "...", "status": "executed",
 "steps": [{"id": "s0", "status": "observed",
            "native_paths": ["pgorm::pipeline::Pipeline::filter"],
            "observation": {"kind": "rows", "rows": [...]}}],
 "cleanup_errors": [], "builds": 0}
```

The independent oracle stays in Python. `replay.native(...)` resets the paired
fixtures, runs the binary against the subject database, runs the ordinary
psycopg `Reference` against the reference database, snapshots both, and assembles
a full Checker-shaped report with a `provenance` block. That document is what
`attribution.provenance` and `attribution.classify` already expect.

This is deliberate. Writing a second reference implementation in Rust would mean
two oracles that can agree with each other and both be wrong. There is one
independent semantics, in Python, and the Rust side is measured against it just
as the Python binding is.

## Encoding input as data

Values are never interpolated into source as SQL, and never as bare literals
where a literal would lose information:

- floats are `f32::from_bits(0x…)` / `f64::from_bits(0x…)`, because the wire
  format is IEEE bits precisely so NaN payloads and signed zero survive
- bytes and MAC addresses are integer arrays, not string escapes
- decimals, UUIDs, temporals and IP networks are parsed from canonical text
- `NaiveDateTime` is parsed with an explicit format: chrono prints a space
  separator but its `FromStr` demands a `T`, so `"…".parse()` would fail
- enum values carry no `Value` variant; they emit as text plus the matching
  `cast_as_type(TypeName…)`, with `.array()` for enum arrays
- every string reaching source — identifier or payload — goes through one Rust
  string-literal escaper, so quotes, backslashes, NUL and non-ASCII in a
  hostile fixture name cannot terminate the literal

Builder operations are emitted as builder calls. Captured SQL is never
substituted for them; a program that cannot be expressed through the named API
is reported as unsupported rather than quietly lowered to a string.

## Parity

`replay.parity(program, python, native)` compares the two runs step by step
through `attribution.parity`, which recomputes both verdicts from retained
evidence rather than trusting status labels. On top of that it compares the
**selected API paths**: the `native_paths` each side recorded per step must
match, so an emitter that reaches a different pgorm method than the binding did
fails validation even when the rows agree.

A reproducible seed is not evidence. Parity always runs the recorded program
against the concrete fixture retained with it.
