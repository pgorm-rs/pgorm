# Python API

This chapter specifies a public Python interface to pgorm, implemented by the
optional `pgorm-python` companion crate using PyO3. The WBS root is `python`;
the governing decision is `[dec:pgorm:python-api]`. These are planned contracts,
not a claim that bindings or distribution artifacts already exist.

The Python surface serves application developers and the generated campaign in
[generative.md](generative.md). The campaign consumes the public bindings;
fixture entities, vulnerable controls and scanner dependencies remain separate.

The immediate integration milestone is `python.direct-builders`: an installed
Python module composes and executes real pgorm builders in process against
PostgreSQL. Neither this milestone nor full package acceptance depends on an
HTTP adapter, an external sqlmap scan or `sqlmap.acceptance`.

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
>
> The companion crate is its own Cargo workspace, which is what keeps Python
> configuration out of ordinary Rust builds. Cargo reads a `patch` table from
> the workspace root and nowhere else, so that separation MUST NOT be allowed
> to change which dependency revisions the extension links: a revision pgorm
> pins MUST reach the companion crate through pgorm's own dependency
> declarations, which travel with pgorm into every workspace that depends on
> it, rather than through a `patch` table, which does not; and the companion
> crate's lockfile MUST resolve it to the pinned revision. A divergence is
> silent — the extension resolves the unpinned crate and behaves differently
> from the library every pgorm test exercises, which for a compiler
> dependency means the bindings emit SQL the Rust API does not — so it MUST
> fail a check in this repository rather than be discovered downstream.

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

> [spec:pgorm:req:python.delegation+1]
> Each exposed operation MUST identify and invoke its corresponding Rust API
> in the Python process through the native extension. Query construction and
> execution MUST NOT require an HTTP adapter, RPC service or subprocess query
> dispatcher. Python MUST compose builder operations directly; selecting a
> fixed query handler and supplying its inputs does not satisfy this contract.
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

> [spec:pgorm:req:python.values+4]
> Conversions MUST preserve supported Rust value variants and nullability in
> both directions. The conversion table MUST cover booleans, signed and
> unsigned integer bounds, floating-point values including signed zero and
> non-finite values, strings, bytes, Decimal, UUID, JSON, date/time values,
> intervals, arrays, qualified enums, PostgreSQL's six built-in range types
> and their multiranges, and the range and multirange types a schema created.
> A created range's kind MUST carry the type's name, quoted and
> schema-qualifiable, and its subtype, which converts each bound. A created
> range converts both ways, a result column of one decoding through its
> subtype's codec to its own kind. A created multirange converts as a value,
> but the driver reports its column's type with no subtype, so reading a
> result column of one MUST raise a decode error, leaving it to be read cast
> to text; an array of a created range or multirange MUST be refused both
> ways.
> An interval is `pgorm.Interval(months, days, microseconds)`, immutable and
> hashable, each field an exact `int` within its width and with its own
> sign, inferred as the `interval` kind, decoded from an `interval` column
> and snapshotted as three integer strings
> (`[spec:pgorm:def:sql.value.interval]`); a `datetime.timedelta` is not one,
> having no months and keeping only its days signed.
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

> [spec:pgorm:req:python.expressions+2]
> Expressions MUST compose comparison, arithmetic, boolean nesting, NULL
> tests, membership including empty sets, literal substring helpers, explicit
> LIKE and SIMILAR TO patterns, function calls and qualified casts through
> Rust builders.
> SQL/JSON's query functions, constructors, `FORMAT JSON` and `IS JSON` MUST
> lower into the corresponding `pgorm_query` builders, each clause applied
> through the builder's own method. Their behaviours MUST be typed per
> function, so a choice PostgreSQL refuses for that function cannot be passed,
> and a choice the Rust builder cannot express MUST raise an error rather than
> be dropped.
> Condition grouping and empty-condition semantics MUST match the named Rust
> API. Python truth testing of a query expression MUST raise an error instead
> of evaluating it as a local boolean or silently dropping part of a predicate.

> [spec:pgorm:req:python.expr-collate]
> `Expr.collate(name, *, schema)` MUST put an expression under a collation
> through Rust's `Expr::collate` (`sql.ast.expr.collate`), the name and the
> schema each an identifier, a `str` or an `Identifier`, refused as
> `ColumnDef.collate` refuses them (`python.schema-collation`): written
> `(expr COLLATE "name")`, the schema first when one is given. It answers an
> `Expr`, so the collated value goes on to be compared, and `asc()` or
> `desc()` over it writes the clause into the sort key, which is where
> PostgreSQL's grammar reads an `ORDER BY`'s collation. A name that is not an
> identifier MUST be refused with `ConstructionError` before anything is
> built. What only the server knows it refuses by SQLSTATE, as a
> `DatabaseError`: a collation that does not exist (`42704`) and a collation
> on a type without one (`42804`).

> [spec:pgorm:req:python.ownership]
> Python query objects MUST have documented reuse, cloning and consumption
> semantics. Reusing one expression in two queries MUST NOT mutate the other
> query or leak parameters. Scope-bound references, binder values, transaction
> handles and registered entity handles MUST reject invalid owners or expired
> scopes before executing SQL. Bindings MUST own their Rust state; they MUST
> NOT extend borrowed lifetimes or bypass Rust typestate using unchecked casts.
> Invalid user operations MUST follow the exception contract, not panic.

> [spec:pgorm:req:python.truth-testing+1]
> Every class the package defines MUST raise `ConstructionError` on Python
> truth testing — `bool()`, `if`, `not`, `and`, `or` — so `if statement:` or
> `statement or fallback` fails where it is written instead of reading the
> object as true: a statement, an expression, `Compiled`, every part either
> is built from (a table, a FROM item, a join kind, a WITH clause, a window
> or frame, a conflict or MERGE action, a sort direction or NULL ordering, an
> SQL/JSON clause, a pipeline and its sources), a schema object, a
> registered entity's or graph's queries and writes, with the views
> generated code wraps them in, and every stream — `ResultStream`,
> `CopyOutStream`, `CopyRowStream` and `Listener` — whose truth would say
> nothing of whether rows remain, so `if rows:` fails as well. Exempt are
> values (`Value`, the range types, `Interval`, `TypeName` and `Identifier`,
> compared with `==` as data), rows, models and what a write answers, pools,
> connections and transactions and the private native handles under them
> and under the streams, registrations and model declarations, and
> exceptions. A test MUST enumerate every class the package's modules
> define, so a class that neither refuses nor is exempt for a stated reason
> fails it.

## Queries for applications

> [spec:pgorm:req:python.statements+10]
> The runtime statement API MUST support SELECT projection, nested filters,
> joins and aliases, grouping/HAVING, ordering, limits and offsets, and
> INSERT/UPDATE/DELETE with supported guards, conflict handling and RETURNING.
> A SELECT MUST reach `sql.ast.select`'s clauses beyond those through the
> Rust builder's own methods: `GroupingElement`'s `set`, `empty`, `rollup`,
> `cube` and `sets` — `sets` taking its first element, so an empty
> `GROUPING SETS` cannot be built — beside the expressions `group_by` takes,
> and `group_by_distinct` (`sql.ast.select.grouping`,
> `sql.ast.select.group-distinct`); `limit_with_ties`, sharing `limit`'s
> slot, and refused with no ORDER BY or beside SKIP LOCKED by Rust's
> `check_tie_limit`, as a `ConstructionError`, when the statement is
> inspected or executed, since the order may be added after the limit, and
> as a method reads it into another statement — a CTE body, a subquery —
> since nothing can order it after (`sql.ast.select.tie-limit`); a
> `TableFunction` FROM item over function calls, `ROWS FROM` over several,
> `WITH ORDINALITY` and its column names, the calls built by `call`, whose
> vocabulary reaches `generate_series` and `unnest` and the record-returning
> `json_to_record`, `jsonb_to_record`, `json_to_recordset` and
> `jsonb_to_recordset`, and any other expression refused
> (`sql.ast.select.table-function`); `column_definition(name, type)`
> appending to the column definition list of the call added last, its type a
> `DataType` or what one is built from, lowered into Rust's
> `column_definition` (`sql.ast.select.column-definitions`); and a table's
> `tablesample`, a
> `SampleMethod`, a percentage and an optional seed, both bound
> (`sql.ast.select.table-sample`).
> A SELECT MUST combine with another `Select` through `union`, `union_all`,
> `intersect`, `intersect_all`, `except_` and `except_all`, Rust's `union`
> with each of the six `UnionType`s (`sql.ast.select.union`): the other
> statement is read into this one as a parenthesised arm, its tie limit
> checked as it is read, and anything but a `Select` is a `TypeError`. The
> arms are written in call order after the receiver's HAVING and WINDOW
> clauses and combine in call order, as Rust's do: `a.union(b).intersect(c)`
> is `(a UNION b) INTERSECT c`, everything before an `INTERSECT` that follows
> a `UNION` or `EXCEPT` parenthesised so PostgreSQL's precedence cannot
> regroup it; the receiver's ORDER BY, row limit and OFFSET, whenever they
> were set, order and bound the combined rows, and its WITH clause precedes
> every arm.
> A SELECT's FROM list and joins MUST take a FROM item that is not a named
> table: `JSON_TABLE`, lowered into `Func::json_table` with each column kind's
> own clauses and behaviours as the Rust builder types them, its first column
> and its alias required so an empty column list or an unnamed item cannot be
> built. A SELECT item MUST be able to call a function over a window, inline or
> named in the statement's WINDOW clause, lowered into `WindowStatement` and
> the `expr_window` family; an inline window may be an `ExtendedWindow(base)`
> built on an entry of the clause, `OVER ( "base" ORDER BY … )`. The WINDOW clause MUST hold every window
> `window(name, window)` declares, in call order, as Rust's
> `SelectStatement::window` appends them: a `Window`, or an
> `ExtendedWindow(base)`, lowered into Rust's `ExtendedWindow`, which builds
> on an earlier entry with an ORDER BY and a frame and has no `partition_by`
> (`sql.ast.window-statement`); anything else declared is a `TypeError`. A
> clause PostgreSQL refuses — a name declared twice, an extended window whose
> base is no earlier entry, has a frame, or already orders its rows when it
> adds an ORDER BY — an item over a name the clause does not declare, and an
> item over an inline `ExtendedWindow` whose base no entry declares, has a
> frame, or already orders its rows when it adds an ORDER BY are refused by
> Rust's `check_windows`, as a `ConstructionError`, where the tie
> limit is checked: when the statement is inspected or executed, and as a
> method reads it into another statement.
> `OVER` MUST attach only to what Rust's `WindowFunction` admits — a function
> call or a JSON aggregate — and a frame MUST follow the Rust frame typestate,
> which types a start by the side of the current row it lies on, so that no
> frame can be built whose end lies on an earlier side than its start, that
> starts at `UNBOUNDED FOLLOWING` or ends at `UNBOUNDED PRECEDING`, or whose
> following start stands alone. Two offsets on one side are not compared, as
> PostgreSQL's grammar does not compare them: `ROWS BETWEEN 1 PRECEDING AND 3
> PRECEDING` builds, and holds no rows. A function PostgreSQL computes only
> over a window MUST NOT be usable without one.
> A RETURNING list MUST be able to read a written row's old and new versions
> and rename them. MERGE MUST follow the Rust typestate: a pending MERGE with no
> WHEN arm MUST NOT be inspectable or executable, each arm's action MUST be
> typed by the kind of row the arm takes, with an explicit error for any
> other, and `merge_action()` MUST be reachable only as the RETURNING list's
> action column. A MERGE MUST be able to read a common table expression and to
> be the body of one.
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

> [spec:pgorm:req:python.entities+3]
> Concrete compiled entity registrations MUST expose their actual Rust entity,
> model, column and ActiveModel operations, including write-state and hook
> semantics. Registration MUST declare its types and supported terminals, the
> declared attributes that change what a key or a join by a relation means —
> a key ending `WITHOUT OVERLAPS`, and each relation's columns, `PERIOD` pair,
> condition type and whether an `on_condition` adds its own condition, a Rust
> closure declared only as present or absent — and each relation's foreign
> key enforcement and deferrability, which change what the server holds the
> key's rows to and not what a join reads. The write terminals that read a
> row's two versions — `UpdateOne::exec_returning_change`,
> `UpdateMany::exec_returning_changes` and
> `Insert::exec_returning_upsert(s)` — MUST be reachable as the statement
> terminals they are in Rust, without ActiveModel hooks, answering a `Change`
> of two models and an `Upserted` that is either the inserted model or the
> updated row's change.
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

> [spec:pgorm:req:python.schema+1]
> Explicit schema APIs MUST expose supported table, index and enum DDL through
> the Rust DDL builders, retaining schema qualification, quoted identifiers
> and type identity. Schema generation for a registered entity MUST invoke the
> corresponding Rust entity schema path. Declaring a Python model or importing
> a module MUST NOT execute DDL automatically. Capabilities MUST distinguish
> runtime DDL construction from registered-entity schema generation.
> Every class `pgorm.schema` defines — its statements, its column and type
> definitions and an entity's generated schema — MUST raise
> `ConstructionError` on Python truth testing, as a query expression does,
> and so MUST `Compiled`, which every inspection returns.

> [spec:pgorm:req:python.schema-collation]
> `ColumnDef.collate(name, *, schema)` MUST give the column its one collation
> through Rust's `ColumnDef::collate` (`sql.ddl.column-def`), the name and the
> schema each an identifier, a `str` or an `Identifier`: written `COLLATE`
> after the type, the schema first when one is given, a later call replacing
> an earlier, since PostgreSQL refuses a column with two. `create_table` and
> `add_column` write it where the column is declared, and `modify_column` on
> the column's retype, `ALTER COLUMN .. TYPE .. COLLATE`, the only place
> PostgreSQL changes a column's collation; every comparison and sort over the
> column then uses it. A name that is not an identifier MUST be refused with
> `ConstructionError` as the column is built. What only the server knows it
> refuses by SQLSTATE, as a `DatabaseError`: a collation that does not exist
> (`42704`) and a collation on a type without one (`42804`).

> [spec:pgorm:req:python.schema-views+2]
> `pgorm.schema` MUST build views, materialized views and `CREATE TABLE AS`
> through pgorm-query's builders (`sql.ddl.view`,
> `sql.ddl.materialized-view`, `sql.ddl.create-table-as`), each a function
> returning a `DDL` statement, under the contract of `python.schema`: a
> relation named by a `Table` without an alias, quoted and schema-qualified as
> it was given, and column names as identifiers. The query is a `Select`,
> handed to the Rust statement as the `SelectStatement` it owns, so its values
> reach the definition as the Rust renderer writes them, as literals, and
> `inspect()` gives an empty parameter list. The query is read as a method
> reads a statement into another (`python.statements`): nothing can order it
> or declare its windows once it is a definition, so a tie limit Rust's
> `check_tie_limit` refuses, as one without ORDER BY, and a WINDOW clause or a
> reference to one that its `check_windows` refuses MUST be refused with
> `ConstructionError` as the function is called, never sent to the server.
> The functions are
> `create_view(name, query, *, columns, or_replace, check_option,
> security_barrier, security_invoker)`, `check_option` being `"local"` or
> `"cascaded"`; `create_recursive_view(name, columns, query, *, or_replace,
> security_barrier, security_invoker)`, which has no check option and refuses
> an empty column list, as PostgreSQL's grammar refuses both;
> `drop_view(name, *, if_exists, cascade)`; `create_materialized_view(name,
> query, *, columns, if_not_exists, with_no_data)`;
> `refresh_materialized_view(name, *, concurrently, with_no_data)`;
> `drop_materialized_view(name, *, if_exists, cascade)`; and
> `create_table_as(name, query, *, columns, if_not_exists, with_no_data)`.
> What the Rust types cannot hold MUST be refused with `ConstructionError` as
> the statement is built, never dropped: a check option other than the two,
> a recursive view without columns, and a refresh both concurrent and with no
> data, which PostgreSQL refuses together (`42601`). The capability manifest
> lists each function as a `schema.` operation needing no registration.
> A recursive view's query MUST be able to read the view: `union` or
> `union_all` joins a non-recursive term to an arm reading it
> (`python.statements`), by its bare name, the name of the `WITH RECURSIVE`
> query PostgreSQL defines the view as, a schema-qualified name looking for
> a relation that does not exist yet (`42P01`).

> [spec:pgorm:req:python.schema-schemas]
> `pgorm.schema` MUST build the statements over a schema through
> pgorm-query's `Schema` (`sql.ddl.schema`), each a function returning a
> `DDL` statement under the contract of `python.schema`, every schema name an
> identifier — a `str` or an `Identifier`, one part, quoted — and none
> schema-qualified, since schemas do not nest. The functions are
> `create_schema(name, *, if_not_exists)`; `drop_schema(name, *names,
> if_exists, cascade)`, which drops every schema it names, in order, the
> first required so the list is never empty; and `rename_schema(name,
> new_name)`, which has no `if_exists`, PostgreSQL's grammar having none for
> it. `inspect()` gives an empty parameter list. A name that is not an
> identifier MUST be refused with `ConstructionError` as the statement is
> built. What only the server knows it refuses by SQLSTATE, as a
> `DatabaseError`: a schema that exists (`42P06`), a `pg_` name (`42939`), a
> missing schema (`3F000`) and a drop under the default `RESTRICT` of a
> schema that holds anything (`2BP01`). The capability manifest lists each
> function as a `schema.` operation needing no registration.

> [spec:pgorm:req:python.schema-domains]
> `pgorm.schema` MUST build the statements over a domain through pgorm-query's
> `Domain` (`sql.ddl.domain`), each domain named as an enum type is — a name,
> or a `TypeName` carrying its schema — and every other name an identifier, a
> `str` or an `Identifier`. `CreateDomain(name, base)`, also spelled
> `create_domain`, is a builder as `CreateTable` is: the base is what
> `ColumnDef` takes for a type, and `collate(name, *, schema)`,
> `default(value)`, `not_null()`, `null()` and `check(condition, *, name)` each
> return a new builder with that clause, the default and the nullability one
> slot each, the later call winning, and the checks in the order added. Each
> `ALTER DOMAIN` action is a function returning a `DDL` statement over the
> domain it names first: `set_domain_default(domain, value)`,
> `drop_domain_default(domain)`, `set_domain_not_null(domain)`,
> `drop_domain_not_null(domain)`, `add_domain_check(domain, condition, *, name,
> not_valid)`, `drop_domain_constraint(domain, name, *, if_exists, cascade)`,
> `rename_domain_constraint(domain, name, new_name)`,
> `validate_domain_constraint(domain, name)` and `rename_domain(domain,
> new_name)`, none taking an `if_exists`, PostgreSQL's grammar having none for
> `ALTER DOMAIN`; and `drop_domain(domain, *domains, if_exists, cascade)` drops
> every domain it names, in order, the first required so the list is never
> empty. A check's condition is a query expression, and
> `pgorm.domain_value()` is the value it tests, the keyword `VALUE`.
> `inspect()` gives an empty parameter list, a default and a check's values
> inlined. A name that is not an identifier, a check that is not an expression
> and an unknown base type MUST be refused as the statement is built, the
> first two with `ConstructionError`. What only the server knows it refuses by
> SQLSTATE, as a `DatabaseError`: a name a type or constraint already has
> (`42710`), a missing domain or constraint (`42704`), a value outside a check
> (`23514`) or a null under `NOT NULL` (`23502`) whether written to a column or
> met among the values already there by `add_domain_check`,
> `set_domain_not_null` or `validate_domain_constraint`, a `COLLATE` on a base
> type without one (`42804`), and a drop under the default `RESTRICT` of a
> domain a column is typed with (`2BP01`). A registered entity's column whose
> type is a domain takes and reads back the values of its base type, its input
> hint the base's. The capability manifest lists `CreateDomain` as
> `schema.create_domain` and each function as a `schema.` operation needing no
> registration, and `domain_value` among the expression constructors.

> [spec:pgorm:req:python.runtime]
> Database I/O MUST have an asyncio-compatible awaitable interface backed by
> reusable Tokio runtime resources and pgorm pools. Native waiting MUST NOT
> block the Python event loop or hold Python interpreter access unnecessarily.
> A runtime, pool or extension build MUST NOT be created per query. Objects
> MUST define interpreter, thread and event-loop ownership; unsupported
> cross-owner use MUST raise a documented error rather than deadlock or panic.

> [spec:pgorm:req:python.connections+3]
> Connection construction MUST expose supported pgorm pool, connection and TLS
> configuration, including certificate verification, without a silent plaintext
> fallback. Pools and checked-out resources MUST support explicit asynchronous
> close/context-manager lifecycles. Queries MUST route through pgorm's normal
> execution and statement-cache machinery, and a pool MUST expose
> `clear_statement_caches()`, a synchronous call that marks every connection's
> cached statements stale as `conn.pool.statement-cache.clear` describes and
> refuses nothing, since it sends nothing. `Pool` MUST take
> `statement_cache_channel`, `None` by default: given a channel, the pool
> listens there and clears on every notification as
> `conn.pool.statement-cache.listen` describes, an empty name or one longer
> than 63 bytes being refused with `ConstructionError`, and closing the pool
> closes the listening connection. `pgorm.STATEMENT_CACHE_CHANNEL` is the
> channel pgorm-migration's runner notifies. Credentials MUST be redacted from
> representations, exceptions and default diagnostic logs.

> [spec:pgorm:req:python.transactions+1]
> Transactions MUST support explicit begin/commit/rollback and asynchronous
> context managers, including supported nested savepoints. A successful block
> commits; an exceptional or cancelled block rolls back or discards the
> connection if its state cannot be recovered. Closed, concurrently misused or
> foreign transaction handles MUST fail explicitly. Bindings MUST preserve
> database error causes and MUST NOT retry writes or failed transactions
> implicitly beyond the documented Rust behavior.
>
> Each transaction and savepoint MUST run inside pgorm's closure transaction
> (`conn.tx.closure`), a task on the runtime serving its commands from inside
> the closure: Python's commit returns `Ok` from it and a rollback `Err`, so
> pgorm issues the `COMMIT` or `ROLLBACK`, and a connection given up mid-command
> drops the closure's future rather than await a rollback on it. A failed
> `COMMIT`, and a failed `ROLLBACK` — which pgorm reports as
> `TransactionError::RollbackFailed` (`conn.tx.closure`) — each reach Python
> with their database error and discard the connection, whose state is then
> unknown; an abandoned frame whose `ROLLBACK` fails discards it too.

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

> [spec:pgorm:req:python.copy+2]
> `COPY … FROM STDIN` and `COPY … TO STDOUT` MUST be reachable on a pool, a
> connection and a transaction through `conn.copy`'s `CopyTrait`, over
> `CopyFrom` and `CopyTo` statements lowered into the Rust statements of
> `sql.ast.copy`. The format MUST be chosen when a statement is built, and an
> option the chosen format or direction does not accept MUST raise a
> construction error when it is set, never render SQL the server refuses.
> `CopyTo` MUST raise a construction error for an `Insert`, `Update`,
> `Delete` or `Merge` source with no RETURNING list, as Rust refuses to make
> it a source, and for a `Select` source whose tie limit or WINDOW clause
> Rust's `check_tie_limit` or `check_windows` refuses, as inspecting it does.
> COPY FROM's source MUST be bytes-like data or a synchronous or asynchronous
> iterable of it; text and any other item MUST be refused, never encoded
> implicitly, and the source MUST be pulled only as the COPY accepts data
> through a bounded queue, so a large source streams rather than
> materialises. A source that raises, or yields an item that is not bytes,
> MUST abort the COPY and propagate its own exception unchanged: outside a
> transaction nothing is written and the connection stays usable; inside one
> the transaction is aborted until it is rolled back. A row the server
> refuses MUST raise the database error carrying its SQLSTATE, and a COPY FROM
> the server refuses before any data moves MUST close its connection, and any
> transaction on it, as `conn.copy` requires. COPY TO MUST be an async
> iterator of `bytes` with bounded buffering, holding the server back when
> nothing reads. A copy MUST reserve its connection or transaction, refusing
> other work on it with a lifecycle error, until a COPY FROM finishes or
> aborts or a COPY TO's end is read; a pool's copy acquires and releases a
> connection of its own. Closing a COPY TO before its end MUST discard its
> connection, and any transaction on it. Cancellation MUST follow
> `python.cancellation`: before finishing it aborts the COPY, and while
> finishing, when the outcome is unknown, it discards the connection or
> transaction.

> [spec:pgorm:req:python.copy.rows]
> `copy_in_rows(statement, rows)` and `copy_out_rows(statement)` MUST run a
> binary COPY as rows of values through `conn.copy.rows` on a pool, a
> connection and a transaction, reserving and releasing what they run on as
> `copy_in` and `copy_out` do (`python.copy`). The statement MUST be a
> `CopyFrom` or `CopyTo` in the `"binary"` format, typed
> `CopyFrom[Literal["binary"]]` or `CopyTo[Literal["binary"]]`, and any other
> format raises a construction error. The columns are the ones pgorm
> describes before the COPY starts; a description the server refuses raises
> its database error and leaves the connection or transaction as a failed
> statement does. `rows` is a synchronous or asynchronous iterable of rows,
> text, bytes and anything else refused with a construction error before
> anything is sent; each row is a sequence of its columns' values in order,
> text or bytes again refused. A value is read as a parameter is — `None` as
> NULL, a plain value by its inferred kind, a `Value` by its own — and
> written against its column's type as a parameter is bound. A row of
> another width, and a value its column's type does not take, MUST raise a
> construction error naming the columns or the column and abort the COPY, as
> anything raised while the data is sent does, so nothing is written. Rows
> are pulled as the COPY takes them, a batch at a time, and the call returns
> the rows written. `copy_out_rows` returns a `CopyRowStream`, an async
> iterator of tuples, each value decoded by its column's type as a result
> column of that type is (`python.results`), fetched as rows arrive with
> bounded buffering; `columns` names the columns. A field that does not
> decode raises a decode error and closes the stream, discarding its
> connection; its end and closing behave as a `CopyOutStream`'s do.

> [spec:pgorm:req:python.listen]
> [spec:pgorm:req:python.listen]
> `pool.listener(capacity=1024)` MUST open a listener through `conn.listen`'s
> `DatabasePool::listener`, a connection of its own with the pool's
> configuration and TLS, outside the pool's size, and closing the pool MUST
> close it. `listen`, `unlisten` and `unlisten_all` change what it receives
> and MUST be usable while another task iterates it; a channel MUST be a
> `str`, and a name longer than 63 bytes MUST raise a construction error
> before anything is sent. The listener MUST be an async iterator of
> `Notification` objects carrying `channel`, `payload` and `process_id`, one
> pull at a time, a second pending pull refused with a lifecycle error. At
> most `capacity` notifications MUST wait unread: one more closes the
> listener, and iteration raises a connection error after yielding those
> waiting; a failed or terminated connection raises its database error the
> same way, and iteration ends after either, with nothing reconnected.
> Closing the listener MUST end iteration, a pending pull included, and
> closing the pool MUST interrupt a pending pull with a lifecycle error.
> Cancellation MUST follow `python.cancellation`: a cancelled pull loses no
> notification and leaves the listener open, while a cancelled `listen`,
> `unlisten` or `unlisten_all` closes it, since the server may have acted on
> a request whose answer never arrived. `notify(channel, payload)` on a pool,
> a connection and a transaction MUST send through pgorm's `notify`, both
> values `str` and bound, so a notification sent in a transaction is
> delivered when it commits and never if it rolls back. `Listener` and
> `Notification` MUST refuse truth testing with a construction error.

> [spec:pgorm:req:python.errors+1]
> The package MUST expose a stable exception hierarchy for construction/type,
> unsupported capability, connection, database, decode, timeout/cancellation
> and lifecycle failures. Where available, database exceptions MUST retain
> SQLSTATE and structured diagnostic fields (`detail`, `hint`, `schema`,
> `table`, `column`, `datatype`, `constraint`) without credentials. A server
> error whose SQLSTATE is a constraint violation pgorm's `SqlError` classifies
> (`[spec:pgorm:sem:error.model.sql-class+4]`) MUST raise the matching
> subclass of `DatabaseError`: `UniqueViolationError` (`23505`),
> `ForeignKeyViolationError` (`23503`), `CheckViolationError` (`23514`),
> `NotNullViolationError` (`23502`) or `ExclusionViolationError` (`23P01`),
> carrying the same fields, so the names the server reported (a domain's in
> `datatype`) are read as from any other `DatabaseError`. Invalid input
> MUST NOT become process termination, a swallowed panic or an ordinary result.
> Any caught internal Rust panic MUST remain an identifiable implementation
> failure and MUST NOT be classified as an expected input rejection.

## Direct builder integration

> [spec:pgorm:req:python.direct-builders]
> An integration suite MUST install a built wheel in a clean Python environment
> and import its declared public module. Using only public bindings, it MUST
> compose SELECT with joins and nested predicates, INSERT, UPDATE and DELETE
> over application tables whose names are supplied at runtime. Within one
> loaded extension build it MUST vary both builder structure and values, inspect
> SQL and tagged parameters, execute against PostgreSQL and check returned rows
> and affected counts against the fixture's expected state. Equivalent Rust
> builder operations MUST establish SQL/parameter parity for these cases.
> The installed suite MUST pass without an HTTP adapter, scanner or query
> dispatcher service and without invoking a compiler or package build per
> query. It MUST exercise both literal and bound value paths. Fixture creation
> MAY use separate test setup; the queries under test MUST use the builders.
> The suite MUST have a documented command and produce failing exit status
> for construction, parity, execution or result mismatches. Broader package
> acceptance MUST retain this suite as required evidence.

## Downstream development and release

> [spec:pgorm:req:python.codegen+2]
> A documented code-generation/build workflow MUST create concrete Python
> wrappers and matching type information for downstream Rust entities and
> selected graph shapes. It MUST preserve qualified names, column mappings,
> enum types and optional decode shapes and detect registration/name collisions.
> A field's Python type follows its reflected Rust type: each Rust value type
> `pgorm.Value` carries maps to the Python type its values decode as — an
> `Interval` field to `pgorm.Interval` whatever the column's field
> qualifier, bound under the `interval` kind, and an array of them to
> `list[pgorm.Interval]` — and a type outside them to `Any`. A field over a
> column whose type is a domain types and binds as the domain's base does,
> the domain's name riding in the Rust entity's column definition: a domain
> over an integer is an `int` bound as `i32`, one over an enum the enum's
> `str`, one over an array a `list`.
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

> [spec:pgorm:req:python.acceptance+1]
> Initial acceptance MUST include the direct-builder integration suite and
> public-API tests against PostgreSQL for an application schema outside the
> harness fixtures, lossless conversion and boundary rejection cases,
> runtime/lifecycle/cancellation tests, compiled
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
