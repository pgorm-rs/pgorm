# Generated pgorm programs

This chapter specifies a Python-driven, grammar-based campaign over the public
PyO3 interface in [python.md](python.md). The WBS root is `generative`; its
governing decision is `[dec:pgorm:generated-orm-testing]`. The deliverable is
generated compositions of pgorm operations and reproducible Rust regressions.

The separate [sqlmap campaign](sqlmap.md) scans fixed adapter queries with an
external detector. Importing its attack inputs does not run that detector and
does not satisfy its acceptance rules. Neither campaign substitutes for Rust
compile-time verification of derives, lifetimes and generic shapes.

## Programs and capabilities

> [spec:pgorm:def:generative.program]
> A generated program is a versioned, typed operation graph plus an ordered
> sequence of effects over declared PostgreSQL fixtures. It identifies values,
> identifiers, model/source references, Rust API paths, result references,
> binder/transaction scopes and expected observations. It is data that can be
> executed through Python bindings and emitted as Rust source, not SQL text or
> a selection from a fixed list of handwritten query routes.

> [spec:pgorm:req:generative.format]
> The portable program format MUST preserve graph structure, operation order,
> exact value tags/bytes, identifier spelling, scope ownership and fixture
> definitions without relying on Python object identity, pickle or executable
> deserialization. References and format/capability versions MUST be validated
> before execution. Unknown operations, incompatible versions and unbounded
> input sizes MUST be explicit errors; loading a program MUST NOT execute it.

> [spec:pgorm:req:generative.grammar]
> Generation MUST vary operations and their composition, including nested
> predicates, joins, projections, query nesting and multi-step CRUD sequences
> that reuse earlier results. Generation MUST track schema types, source
> availability, nullability and binder/transaction scopes so valid-program
> campaigns produce executable programs within declared depth/size budgets.
> Invalid-input campaigns MUST be separately labeled with the rejection being
> checked. Changing only strings in fixed query templates is insufficient.

> [spec:pgorm:req:generative.matrix]
> A version-controlled coverage matrix MUST map generated operations to actual
> Rust APIs and enumerate required value/context combinations, both literal and
> bound paths, runtime descriptors and compiled entity/graph shapes. It MUST
> record unsupported or compile-only paths with reasons. Full profiles MUST
> cover each family below, with separate entries wherever the builders differ.
> A call to pgorm-query MUST NOT establish coverage of a bypassed entity,
> ActiveModel, graph or pipeline API.

| Family | Required variations |
| --- | --- |
| CRUD | SELECT, insert values, update values and guards, delete guards, conflict/RETURNING, empty batches, omitted/NULL/set writes |
| Conditions | Nested AND/OR/NOT, empty membership/disjunction, NULLs, tenant guards and predicate grouping |
| Graph | Required/optional joins, self-joins, hostile aliases, absent sources, model decode, cursor bounds and duplicate sort keys |
| Pipeline | Literal/binder paths, projections, grouping/HAVING, windows, joins, composed sources, select_sources and set operations |
| Names | Schema/table/column/alias, ORDER/GROUP selection, function/type/qualified-enum names and enum DDL |
| Substitution/patterns | Owned templates with comments/quotes/dollar strings/repeated slots, literal substring helpers and explicit LIKE |
| Types | Numeric bounds, float edge cases, nullability, arrays, JSON, Decimal, UUID, temporal and enum variants |
| Sequences | Store/read/reuse as value or identifier, commit/rollback/savepoint, repeated execution and stream cancellation |

> [spec:pgorm:req:generative.corpus]
> Hostile-input generation MUST include quotes, comments, statement separators,
> dollar strings, backslashes, Unicode, wildcard characters and numeric/type
> boundaries in addition to random values. An optional sqlmap importer MUST
> record upstream revision, content hashes, applicable license notices and
> transformation rules. It MUST instantiate supported contextual placeholders
> and boundaries explicitly, and inventory unsupported templates with reasons.
> Payload strings MUST remain input values or identifiers, never silently
> become intentional boolean/raw-SQL operations in a protected program.
> Corpus availability MUST NOT require completing or running the HTTP scanner.

## Native execution

> [spec:pgorm:req:generative.execution]
> The executor MUST dispatch generated operations through the public Python
> bindings to their declared pgorm APIs, preserving inputs and path selection.
> Test-only adapters may register concrete fixture entity and graph types but
> MUST NOT replace their implementation or the public binding conversions.
> Per-operation evidence MUST identify the actual native path and observation;
> missing/inactive dispatch MUST fail rather than report unexercised coverage.

> [spec:pgorm:req:generative.build-amortization]
> The runtime campaign MUST reuse an installed extension, runtime and bounded
> connection pools across generated programs. Compilation MUST occur only when
> the source revision, native features, ABI or registered type/shape set changes.
> Runtime workers MUST NOT invoke a compiler/build tool per program. Reports
> MUST separate build counts/times from generation, execution and oracle costs
> and retain the extension content identity used throughout the run.

> [spec:pgorm:req:generative.fixtures]
> Each program and each shrink/replay attempt MUST start from a known fixture
> state that can be reproduced independently. Fixtures MUST include multiple
> tenants, matching/nonmatching rows, protected write sentinels, NULLs,
> duplicate sort keys, optional joins, schema collisions and quoted/enum names.
> State persists between operations within a generated sequence. Reference
> execution and subject execution MUST receive equivalent independent baselines.
> PostgreSQL image/version, encoding, collation, timezone, search_path, string
> mode and statement/lock timeouts MUST be recorded.

> [spec:pgorm:req:generative.isolation]
> Live campaigns MUST provision disposable state with a restricted execution
> role and keep administrative credentials outside generated programs. Ordinary
> arbitrary connection URLs MUST NOT be accepted as campaign targets. Rollback
> alone MUST NOT contain deliberately injected SQL. Process/resource budgets,
> cancellation and cleanup MUST cover workers, reference execution and native
> work; cleanup failure is incomplete evidence, never success. Controls and
> replay programs MUST use harmless fixture-local effects and synthetic data.

## Independent checks

> [spec:pgorm:req:generative.oracles]
> Every executable generated case MUST declare a meaningful oracle: an
> independent reference model/query, a justified metamorphic relation or a
> specific data/state invariant. Reference SQL MUST be constructed independently
> of the subject renderer with bound values and independently handled names.
> A comparison with the same generated SQL, parser success, scanner silence or
> absence of an exception alone MUST NOT establish correctness. Unsupported
> oracle semantics MUST be reported as uncovered/incomplete rather than passed.

> [spec:pgorm:req:generative.comparison]
> Oracles MUST account for PostgreSQL NULL/three-valued logic, duplicate rows,
> ordering only where specified, scalar/array/enum types, numeric and temporal
> semantics, affected-row counts and final fixture state. Normalization MUST
> be explicit and MUST NOT erase a defect such as a missing row, altered value,
> lost type tag, wrong optional join or broadened tenant predicate. Expected
> invalid-input or database errors MUST identify their expected category/cause;
> arbitrary errors MUST NOT satisfy an expected rejection.

> [spec:pgorm:req:generative.controls]
> Test-only vulnerable implementations and deliberately incorrect outcomes MUST
> demonstrate that each oracle family detects its intended defect. Required
> controls include escaped-value/identifier failure, predicate broadening,
> wrong bind order/type, decode mismatch and missing state effects. They MUST
> remain independent of the protected implementation. Undetected controls,
> failed baselines or inactive APIs MUST make the relevant campaign incomplete
> or invalid, not clean. Control artifacts MUST never enter public packages.

> [spec:pgorm:req:generative.attribution]
> Findings MUST preserve evidence sufficient to distinguish generator/oracle
> defects, binding conversion/dispatch defects and underlying pgorm defects.
> Native Rust replay MUST be used to investigate this boundary; failure to
> reproduce natively MUST NOT erase the original Python failure. Detection
> MUST NOT automatically patch the library, weaken expectations or close WBS
> work. Changes and named regressions remain separately reviewed work.

## Shrinking and replay

> [spec:pgorm:req:generative.shrink]
> Shrinking MUST reduce operation sequences, expression structure, fixtures and
> values while retaining valid dependencies/scopes and the original failure
> predicate. Each candidate MUST run from its own known baseline. A syntax,
> unsupported-capability or fixture error MUST NOT replace the original defect.
> Shrink budgets and the final predicate MUST be recorded; interruption MUST
> retain the original program and best reproducer found without claiming a
> globally minimal result.

> [spec:pgorm:req:generative.replay]
> Each finding MUST retain the portable program and a runnable Python replay
> plus Rust source using the same named pgorm API paths, fixture definitions,
> value tags, ordering and assertion. The Rust emitter MUST encode input as
> data without source-code interpolation vulnerabilities and MUST NOT replace
> builder operations with captured raw SQL. Generated tests MUST compile/run
> independently of Python/sqlmap. Pure Python-boundary failures retain their
> Python reproducer and native comparison even when the Rust comparison passes.

> [spec:pgorm:req:generative.replay-parity]
> Representative generated programs MUST run through Python and emitted Rust
> against equivalent fixtures and compare observations and selected API paths.
> This parity check MUST cover all executable grammar families, including
> scope-bound binders and compiled model/graph types. Emitter or binding drift
> MUST fail validation; a reproducible seed alone MUST NOT substitute for the
> recorded program, dependencies and concrete fixture state.

> [spec:pgorm:req:generative.compile-suite]
> A separate bounded Rust source-generation suite MUST vary entity derives,
> codegen inputs, generic instantiations, graph tuple shapes and compile-time
> ownership/type boundaries that installed bindings cannot vary. Positive and
> negative cases MUST distinguish expected compile rejection from generator or
> toolchain failure. Programs SHOULD be batched per temporary crate/build where
> compatible. Compile coverage MUST be reported separately from runtime counts.

## Campaigns and acceptance

> [spec:pgorm:req:generative.profiles]
> Versioned smoke and full profiles MUST declare operation/shape coverage,
> generation and fixture limits, seed policy, worker counts, execution/shrink
> budgets and control requirements. Full profiles MUST include both ordinary
> and hostile data across the full matrix. Construction-only throughput runs,
> live database runs, invalid-input tests and compile suites MUST be separately
> identified; one million constructor calls MUST NOT be reported as one million
> independently checked database programs.

> [spec:pgorm:req:generative.verdict]
> Every scheduled program/control MUST end with a recorded pass, defect,
> expected-rejection, invalid-control or incomplete result. Missing workers,
> crashes, unexpected panics/errors, deadlines, absent/malformed artifacts,
> skipped work, empty discovery and cleanup failures MUST make the command fail.
> Aggregate success requires satisfied profile coverage, successful controls
> and all scheduled work accounted for. Runtime, compile, binding and external
> sqlmap claims MUST remain separately identifiable.

> [spec:pgorm:req:generative.artifacts]
> Reports MUST record source revision/dirty content identity, Python/pgorm/PyO3
> and dependency pins, extension hash, capability/grammar/corpus/profile versions,
> PostgreSQL identity/settings, seeds, expected/executed work, native call
> evidence, observations, oracle decisions and timing/build counts. Failures
> MUST retain original and reduced programs, fixture setup and replay commands.
> Data MUST be synthetic and credentials omitted. A fresh run MUST NOT inherit
> old passing evidence or unrecorded generator example-database state.

> [spec:pgorm:req:generative.ci]
> Dedicated CI MUST provide bounded pull-request smoke and scheduled/manual
> full runtime and compile campaigns using the same local runner and pinned
> artifacts. Fast runner tests MUST demonstrate failure for empty/missing work,
> dead dispatch, undetected controls, bad oracles, bad replay, timeout/cancel,
> malformed results and cleanup failure. Slow campaigns MUST stay outside
> pre-commit checks. Relevant source and test paths MUST be registered in nspec.

> [spec:pgorm:req:generative.acceptance]
> Initial acceptance MUST include a full runtime report with all required
> coverage/control obligations met, Python/Rust replay parity, bounded compile
> suite results and named regressions for resolved discoveries. A separate
> stress demonstration MUST execute at least 1,000,000 distinct generated
> programs through one unchanged native build identity with zero per-program
> builds, recording which programs reached PostgreSQL and an oracle. Throughput
> MUST be measured without imposing an unsupported speed target. This stress
> evidence does not replace full live coverage or prove all ORM programs safe.

## Design references

- [Hypothesis stateful testing](https://hypothesis.readthedocs.io/en/latest/stateful.html)
  provides a candidate implementation for generating action sequences and
  shrinking them. The portable format remains independent of its internal data.
- [sqlmap payload metadata](https://github.com/sqlmapproject/sqlmap/blob/master/data/xml/payloads/boolean_blind.xml)
  documents contextual payload templates; imports require pinned content and
  explicit supported transformations.

The campaign is test infrastructure. A normal Python user neither needs its
program format nor its runner to construct and execute pgorm queries.
