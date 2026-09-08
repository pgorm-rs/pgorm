# Python API

This chapter specifies a public Python interface to pgorm, implemented by the
optional `pgorm-python` companion crate using PyO3. The WBS root is `python`;
the governing decision is `[dec:pgorm:python-api]`. These are planned contracts,
not a claim that bindings or distribution artifacts already exist.

The Python surface serves application developers and the generated campaign in
[generative.md](generative.md). The campaign consumes the public bindings;
fixture entities, vulnerable controls and scanner dependencies remain separate.

## Package boundary

> [spec:pgorm:def:python.api]
> The Python API is a Python package backed by a native PyO3 extension over
> pgorm's Rust query construction, execution and decoding APIs. It includes
> runtime schema descriptors for ordinary application schemas and an extension
> mechanism for concrete compiled Rust entity and graph types. Python programs
> compose operations and values at runtime within the installed capability set.

> [spec:pgorm:req:python.optional]
> Python support MUST live in an opt-in companion crate. Ordinary pgorm builds,
> default workspace builds and Rust library users MUST NOT require Python,
> PyO3, a Python linker configuration or scanner dependencies. Building the
> extension MUST be explicit. Public artifacts MUST exclude fixture models,
> deliberately vulnerable controls and campaign-only dependencies.

> [spec:pgorm:req:python.package]
> The package MUST declare its Rust crate, Python distribution, import module,
> pgorm compatibility and supported Python/platform matrix in version-controlled
> metadata. Maturin builds MUST produce an installable wheel and source
> distribution. The installed module MUST expose package, pgorm, binding and
> enabled-feature identities. A clean environment MUST be able to import and
> use the wheel without a Rust toolchain; source builds may require one.
> Distribution-name availability and the chosen ABI policy MUST be established
> before release; this specification does not assert a reserved registry name.

## Faithful construction and conversion

> [spec:pgorm:req:python.delegation]
> Each exposed operation MUST identify and invoke its corresponding Rust API.
> Bindings MUST NOT implement a second SQL renderer, escaping algorithm,
> placeholder substitution engine or query optimizer. Python conveniences MUST
> lower into documented Rust operations. A dynamic statement path MUST NOT be
> presented as execution of an EntityTrait, ActiveModel or typed graph path.

> [spec:pgorm:req:python.capabilities]
> A versioned capability manifest MUST enumerate supported operations, value
> types, result forms, feature requirements, corresponding Rust APIs and
> compiled entity/graph registrations. Unsupported capabilities MUST fail
> explicitly before execution. Runtime-generated compositions within those
> capabilities MUST NOT invoke rustc, Cargo, Maturin or a code-generation build.

> [spec:pgorm:req:python.values]
> Conversions MUST preserve supported Rust value variants and nullability in
> both directions. The conversion table MUST cover booleans, signed and
> unsigned integer bounds, floating-point values including signed zero and
> non-finite values, strings, bytes, Decimal, UUID, JSON, date/time values,
> arrays and qualified enums where the corresponding Rust feature exists.
> Python bool MUST NOT silently select the integer path. Integer overflow,
> unsupported types and ambiguous conversions MUST raise explicit exceptions;
> values MUST NOT silently stringify, wrap, truncate or become NULL.

> [spec:pgorm:req:python.value-tags]
> Explicit constructors MUST retain distinctions that Python values alone
> cannot encode: SQL NULL versus JSON null, typed NULLs and empty arrays,
> integer widths, enum type identity and temporal variants. Decimal conversion
> MUST NOT pass through float; temporal conversion MUST preserve the declared
> timezone policy and precision or explicitly reject an unrepresentable value.
> Serialization used by testing MUST preserve these tags and special values.

> [spec:pgorm:req:python.input-boundaries]
> Values, identifiers, qualified type names, typed structural choices and raw
> SQL MUST have distinct constructors. Accepted strings MUST reach the selected
> Rust API unchanged, including quotes, percent signs, backslashes and Unicode.
> Python or PostgreSQL representation limits MUST be explicit errors, not
> silent normalization. Sort directions, limits and offsets MUST use the real
> typed Rust boundaries. Literal-value and binder-value paths MUST remain
> separately expressible; bindings MUST NOT replace one with the other.

> [spec:pgorm:req:python.expressions]
> Expressions MUST compose comparison, arithmetic, boolean nesting, NULL
> tests, membership including empty sets, literal substring helpers, explicit
> LIKE patterns, function calls and qualified casts through Rust builders.
> Condition grouping and empty-condition semantics MUST match the named Rust
> API. Python truth testing of a query expression MUST raise an error instead
> of evaluating it as a local boolean or silently dropping part of a predicate.

> [spec:pgorm:req:python.ownership]
> Python query objects MUST have documented reuse, cloning and consumption
> semantics. Reusing one expression in two queries MUST NOT mutate the other
> query or leak parameters. Scope-bound references, binder values, transaction
> handles and registered entity handles MUST reject invalid owners or expired
> scopes before executing SQL. Bindings MUST own their Rust state; they MUST
> NOT extend borrowed lifetimes or bypass Rust typestate using unchecked casts.
> Invalid user operations MUST follow the exception contract, not panic.

## Queries for applications

> [spec:pgorm:req:python.statements]
> The runtime statement API MUST support SELECT projection, nested filters,
> joins and aliases, grouping/HAVING, ordering, limits and offsets, and
> INSERT/UPDATE/DELETE with supported guards, conflict handling and RETURNING.
> Schema/table/column names MUST be runtime inputs; shipped fixture names MUST
> NOT be required. Execution and SQL inspection MUST use the same Rust builder
> state, with SQL text and tagged bind values distinguishable in inspection.

> [spec:pgorm:req:python.models]
> Applications MUST be able to declare schema-qualified table/model descriptors
> with columns, types, nullability and keys without rebuilding the extension.
> Their query and CRUD operations MUST lower into the documented runtime Rust
> statement paths and return Python records with declared field identities.
> Omitted, explicit NULL and assigned values MUST remain distinguishable on
> writes. Unknown fields, incompatible values and duplicate output identities
> MUST be explicit errors. Dynamic descriptors MUST NOT claim to instantiate
> Rust derives, EntityTrait implementations or ActiveModelBehavior hooks.

> [spec:pgorm:req:python.entities]
> Concrete compiled entity registrations MUST expose their actual Rust entity,
> model, column and ActiveModel operations, including write-state and hook
> semantics. Registration MUST declare its types and supported terminals.
> Python MUST NOT fabricate new generic instantiations at runtime. A supported
> registration mechanism MUST permit downstream application entities; the
> public package MUST NOT be limited to this repository's test fixtures.

> [spec:pgorm:req:python.graph]
> Compiled graph registrations MUST invoke SelectGraph with their declared
> entity tuple, join slot kinds and decoding terminals. Python may vary filters,
> source aliases, ordering and supported cursor bounds within that shape.
> Required/optional rows, absent joined sources and decode errors MUST remain
> distinct. Unsupported entity combinations and arities MUST be reported in the
> capability manifest and rejected, never lowered silently to a different API.

> [spec:pgorm:req:python.pipeline]
> The pipeline API MUST compose runtime sources, joins, filters, projections,
> derive, grouping/aggregation, windows, ordering, take and supported set
> operations through pgorm::pipeline. Literal and binder paths MUST remain
> selectable. Bound expressions MUST be lowered within valid Rust binder
> scopes; Python ownership MUST NOT erase their scope discipline. Registered
> select_sources terminals MUST preserve Rust model/optional-row decoding.

> [spec:pgorm:req:python.raw]
> Raw SQL MUST be an explicit application API with a declared SQL template and
> separate values, retaining pgorm's execution semantics. An owned-template
> substitution API, if exposed, MUST invoke the Rust lexer/substitution path.
> Neither API may be an implicit fallback for unsupported builder operations.
> Campaign-generated protected queries MUST obey their stricter raw-slot rules.

## Execution and lifecycle

> [spec:pgorm:req:python.schema]
> Explicit schema APIs MUST expose supported table, index and enum DDL through
> the Rust DDL builders, retaining schema qualification, quoted identifiers
> and type identity. Schema generation for a registered entity MUST invoke the
> corresponding Rust entity schema path. Declaring a Python model or importing
> a module MUST NOT execute DDL automatically. Capabilities MUST distinguish
> runtime DDL construction from registered-entity schema generation.

> [spec:pgorm:req:python.runtime]
> Database I/O MUST have an asyncio-compatible awaitable interface backed by
> reusable Tokio runtime resources and pgorm pools. Native waiting MUST NOT
> block the Python event loop or hold Python interpreter access unnecessarily.
> A runtime, pool or extension build MUST NOT be created per query. Objects
> MUST define interpreter, thread and event-loop ownership; unsupported
> cross-owner use MUST raise a documented error rather than deadlock or panic.

> [spec:pgorm:req:python.connections]
> Connection construction MUST expose supported pgorm pool, connection and TLS
> configuration, including certificate verification, without a silent plaintext
> fallback. Pools and checked-out resources MUST support explicit asynchronous
> close/context-manager lifecycles. Queries MUST route through pgorm's normal
> execution and statement-cache machinery. Credentials MUST be redacted from
> representations, exceptions and default diagnostic logs.

> [spec:pgorm:req:python.transactions]
> Transactions MUST support explicit begin/commit/rollback and asynchronous
> context managers, including supported nested savepoints. A successful block
> commits; an exceptional or cancelled block rolls back or discards the
> connection if its state cannot be recovered. Closed, concurrently misused or
> foreign transaction handles MUST fail explicitly. Bindings MUST preserve
> database error causes and MUST NOT retry writes or failed transactions
> implicitly beyond the documented Rust behavior.

> [spec:pgorm:req:python.cancellation]
> Cancellation, timeout, iterator abandonment and pool shutdown MUST release
> owned resources and prevent an uncertain connection from returning to the
> pool as healthy. Cancellation MUST NOT be reported as a successful query,
> nor promise rollback of a write whose outcome is unknown. Tests MUST show
> that cancellation and shutdown terminate within declared budgets and leave
> no background work using a closed interpreter or transaction.

> [spec:pgorm:req:python.results]
> Query terminals MUST distinguish rows, no row, affected counts and decode
> failure, preserving field identity, value tags and optional joined models.
> Streaming MUST offer an async iterator with bounded buffering and documented
> connection ownership until completion or close. Decoding MUST use the
> documented Rust decode path; a database or decode error MUST NOT become an
> empty result, None or a missing joined row.

> [spec:pgorm:req:python.errors]
> The package MUST expose a stable exception hierarchy for construction/type,
> unsupported capability, connection, database, decode, timeout/cancellation
> and lifecycle failures. Where available, database exceptions MUST retain
> SQLSTATE and structured diagnostic fields without credentials. Invalid input
> MUST NOT become process termination, a swallowed panic or an ordinary result.
> Any caught internal Rust panic MUST remain an identifiable implementation
> failure and MUST NOT be classified as an expected input rejection.

## Downstream development and release

> [spec:pgorm:req:python.codegen]
> A documented code-generation/build workflow MUST create concrete Python
> wrappers and matching type information for downstream Rust entities and
> selected graph shapes. It MUST preserve qualified names, column mappings,
> enum types and optional decode shapes and detect registration/name collisions.
> Compilation occurs when those definitions or enabled shapes change, not
> when a user constructs another query over the installed types. Generated
> modules MUST declare compatibility with the binding registry and pgorm build.

> [spec:pgorm:req:python.typing]
> Public Python APIs MUST ship typing information and py.typed, with callable
> signatures, awaitable/iterator results, optional values and error behavior
> documented. Static-checker examples and runtime signature tests MUST agree
> with the installed extension. Documentation MUST show an ordinary schema,
> CRUD, joins, a transaction, a pipeline, streaming and the optional compiled
> entity workflow, without requiring the security harness.

> [spec:pgorm:req:python.distribution]
> CI MUST build and install wheels and a source distribution in clean
> environments for the declared support matrix, exercising import, native
> loading, a real database operation, TLS configuration and typing artifacts.
> Public dependencies, licenses and linked native components MUST be included
> in package metadata/notices. Platform, Python ABI and free-threading support
> MUST be claimed only for tested combinations. Artifacts MUST be ready for
> review without uploading to package registries as part of ordinary tests.

> [spec:pgorm:req:python.acceptance]
> Initial acceptance MUST include public-API tests against PostgreSQL for an
> application schema outside the harness fixtures, lossless conversion and
> boundary rejection cases, runtime/lifecycle/cancellation tests, compiled
> entity/graph registration tests and install/type-check tests of built
> artifacts. Rust/Python parity tests MUST cover every claimed API family and
> execution path. Default Rust builds MUST still work without Python tooling.
> nspec MUST track Rust bindings, Python sources and their tests; documentation
> alone MUST NOT count as implementation or verification evidence.

## Design references

- [PyO3 class restrictions](https://pyo3.rs/main/class#restrictions) explain why
  Rust generic entity types need concrete registrations.
- [PyO3 parallelism](https://pyo3.rs/main/parallelism) describes interpreter
  attachment when native work runs independently.
- [Maturin](https://www.maturin.rs/) documents native Python package builds.

Dependency versions and support matrices are implementation deliverables;
these references do not select moving upstream revisions as release pins.
