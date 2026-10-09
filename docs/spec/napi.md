# JavaScript API (Node-API)

This chapter specifies pgorm's interface to JavaScript, implemented by the
optional `pgorm-napi` companion crate: a native addon built with the Neon
framework against Node-API, which both Node.js and Deno load. The WBS root is
`napi`. It mirrors [python.md](python.md)'s structure in JavaScript
conventions — camelCase, a Promise for anything asynchronous, typed errors —
and these rules grow with it. What exists today is the foundation the rest of
the binding is built on: the loading model, the runtime that runs pgorm's
asynchronous work, the contract by which that work settles a Promise, and a
process's clean exit in both runtimes.

## Package boundary

> [spec:pgorm:def:napi.api+1]
> The JavaScript API is an ES module, `pgorm-napi/lib/index.js`, backed by a
> native Node-API addon over pgorm's Rust query construction, execution and
> decoding APIs. The module loads the addon, hands it the error and value
> classes it rejects with and builds values from, exports them, and is what an
> application imports; the addon is not imported directly.

> [spec:pgorm:req:napi.optional]
> JavaScript support MUST live in an opt-in companion crate. Ordinary pgorm
> builds, default workspace builds and Rust library users MUST NOT require
> Node.js, Deno, Neon or a Node-API linker configuration. The companion crate
> is its own Cargo workspace, and the rule `python.optional` states
> for pinned dependency revisions holds for it unchanged: a revision pgorm
> pins MUST reach the addon through pgorm's own dependency declarations, never
> a `patch` table, and the crate's committed lockfile MUST resolve it to the
> pinned revision.

## Targeting and loading

> [spec:pgorm:req:napi.loading]
> The addon MUST target Node-API version 6 — the lowest that carries
> per-instance data, which the shared channel, instance-local values and the
> dropped-handle queue rest on — and MUST NOT link against a particular
> runtime's executable: its Node-API symbols are resolved from the host process
> when it loads, so the one build serves Node.js and Deno.
>
> The built library is copied to `lib/pgorm_napi.node`, beside the module that
> loads it, and kept out of version control. `lib/index.js` loads it through a
> CommonJS `require` made with `node:module`'s `createRequire`, the one loader
> both runtimes give a `.node` file. Node.js needs no flag to load it. Deno
> MUST be granted `--allow-ffi`, to open a native library, and `--allow-read`,
> to resolve its path; `--allow-net` is not needed and does not confine the
> addon. Native code runs outside Deno's permission checks, so granting
> `--allow-ffi` to a program that loads the addon trusts it with everything
> the process can do, the network included.

## Runtime and threading

> [spec:pgorm:req:napi.runtime]
> pgorm's asynchronous work MUST run on one multi-threaded Tokio runtime per
> process, built when the first instance of the addon loads and shared by
> every instance after it — the main thread's and each worker thread's. A
> runtime MUST NOT be created per call or per query. A JavaScript thread MUST
> NOT block on the runtime. The runtime is never shut down: its idle threads
> hold nothing a JavaScript event loop waits on, the process's exit ends them,
> and shutting it down with an instance would cancel the work of the instances
> still running.

> [spec:pgorm:req:napi.promises]
> Every operation that does I/O MUST return a Promise and MUST NOT throw
> synchronously: an argument that cannot be used rejects the Promise like any
> other failure. The work runs on the runtime, and its outcome MUST settle the
> Promise on the JavaScript thread of the instance that started it, through
> that instance's shared channel. Each operation in flight MUST hold a
> reference that keeps its event loop alive, and MUST release it once its
> settlement is queued, so that concurrent operations each settle with their
> own outcome and a loop with nothing in flight is free to finish. An
> operation MUST NOT leave its promise to Neon's drop queue, which rejects a
> promise whose `Deferred` was dropped unsettled: the queue holds no event
> loop open, and Node.js finishes a loop with its deliveries still pending
> where Deno delivers them first, so a promise left to it may never settle.

> [spec:pgorm:req:napi.errors+1]
> Failures MUST reject with instances of the module's error classes, all
> derived from `PgormError` and `Error`: `DatabaseError` when PostgreSQL
> reports an error, carrying its SQLSTATE as `sqlstate` with its severity and
> optional diagnostic fields (`detail`, `hint`, `schema`, `table`, `column`,
> `constraint`, each a string or `null`); `ConnectionError` when the server
> cannot be reached or the connection breaks; `ConstructionError` for an
> argument that cannot become the value pgorm sends; `DecodeError` for a result
> that cannot become the JavaScript value asked for; `LifecycleError` for a
> pool, connection, transaction or stream used after it closed or while
> another operation holds it; `TimeoutError` when no connection becomes free
> within a pool's acquire budget; `InternalError` when pgorm
> or the binding fails in a way no input should cause, a panic on the runtime
> above all, which MUST reject its promise rather than leave it pending or be
> classified as an expected input rejection. An error from PostgreSQL
> anywhere in a failure's cause chain makes it a `DatabaseError`, so a refusal
> during connection carries its SQLSTATE too. Credentials MUST NOT appear in
> an error, nested causes included. A failure is captured as data on the
> runtime thread and becomes an error object on the JavaScript thread, built
> by a factory the module registers with its instance; an instance with none
> registered rejects with an `Error` of the same name and fields.

## Values

Every kind of value pgorm's `Value` holds crosses into and out of JavaScript
with one declared JavaScript type, and the one PostgreSQL type pgorm has no
variant for, `interval`, crosses as well. Precision is the first rule: a value
reaches JavaScript exactly or not at all.

| Kind | PostgreSQL | JavaScript |
| --- | --- | --- |
| `bool` | `boolean` | `boolean` |
| `i8`, `i16`, `i32`, `u32` | `"char"`, `int2`, `int4`, `oid` | `number` |
| `i64`, `u64` | `int8` | `bigint` |
| `f32`, `f64` | `float4`, `float8` | `number` |
| `text`, `char` | `text`, `varchar`, `bpchar`, `name` | `string` |
| `bytes` | `bytea` | `Uint8Array` |
| `json` | `json`, `jsonb` | `null`, boolean, number, `bigint`, string, array, plain object |
| `decimal` | `numeric` | the module's `Decimal` |
| `uuid` | `uuid` | the module's `Uuid` |
| `date`, `time`, `datetime`, `datetime_utc` | `date`, `time`, `timestamp`, `timestamptz` | `Temporal.PlainDate`, `PlainTime`, `PlainDateTime`, `Instant` |
| `interval` | `interval` | the module's `Interval` |
| `ipnetwork`, `mac_address` | `inet`/`cidr`, `macaddr` | `string`, six-byte `Uint8Array` |
| `vector` | pgvector's `vector` | `Float32Array` |
| a built-in range or multirange | `int4range` … `tstzmultirange` | the module's `Range`, `Multirange` |
| an enum | its type | `string` |
| a created range | its type | `Range`, its kind a `CreatedRange` |

> [spec:pgorm:req:napi.values]
> Every kind of pgorm `Value`, and `interval`, MUST cross into and out of
> JavaScript as the one JavaScript type the table above declares for it, and
> SQL NULL as `null`, both ways. A value MUST reach JavaScript exactly or not
> at all: `int8` decodes as a `bigint`, never a number; `numeric` as a
> `Decimal`, whose text keeps the value's scale, within pgorm's range of a
> 96-bit coefficient and 28 fractional digits; `float4` widened exactly; a
> JSON number exactly, an integer literal past `Number.MAX_SAFE_INTEGER` as a
> `bigint` and a number serde_json would read as another refused. A value
> with no exact JavaScript form — a `numeric` past pgorm's range, `NaN` or
> infinite; a date or timestamp outside the years -9999 to 9999 pgorm holds,
> or infinite; `24:00`; an array of more than one dimension or not starting
> at 1; a type the table does not list — MUST be a `DecodeError`, never a
> rounded value, a string, `null` or a missing column. A domain decodes as
> the type it is built over.
>
> In the other direction a value MUST become the pgorm value it declares or
> be refused with a `ConstructionError`: an integer outside its kind's range,
> a number past 2^53 - 1 declared as an integer, an `f32` that narrowing
> would change, a string holding a lone surrogate, a `Decimal` outside pgorm's
> range or in exponent notation, a `Decimal` made from a number, which may
> already be rounded, and statement text holding NUL. A `Decimal` MUST NOT
> become a number implicitly. Parameters are bound, never interpolated,
> through pgorm's `ValueHolder`, which writes each against the type
> PostgreSQL inferred for its placeholder and refuses a value that type
> cannot receive; its coercions — an integer to any numeric type, a float
> to `float4` rounding as PostgreSQL's own cast does — are pgorm's, and a
> refusal names the parameter and the codec's reason.

> [spec:pgorm:req:napi.temporal]
> Dates and times MUST be Temporal's: `date` a `Temporal.PlainDate`, `time` a
> `Temporal.PlainTime`, `timestamp` a `Temporal.PlainDateTime` and
> `timestamptz` a `Temporal.Instant`, a `PlainDate` or `PlainDateTime` in
> another calendar binding as the same day. A `timestamptz` is an instant:
> PostgreSQL keeps no zone with it and sends it in binary as microseconds from
> 2000-01-01 UTC, so the instant read MUST NOT depend on the session's
> `TimeZone`, which governs only its text rendering and the reading of text
> without an offset. A `ZonedDateTime` would invent a zone on the way out and
> drop one on the way in, so it is refused as a parameter, as a `Date` is.
> A `PlainDateTime` MUST NOT bind to `timestamptz`, nor an `Instant` to
> `timestamp`, in an array or a range bound included: pgorm writes both as
> microseconds and would read either as UTC, where PostgreSQL's own cast
> between the two reads the wall clock in the session's zone.
>
> PostgreSQL keeps microseconds and Temporal nanoseconds. Reading MUST be
> exact, and a value written with a nonzero digit below the microsecond MUST
> be a `ConstructionError`, never truncated or rounded by the binding.
>
> `interval` MUST keep PostgreSQL's three independently signed fields —
> months, days and microseconds — as the module's `Interval`, because
> `Temporal.Duration` requires every field to share one sign and so cannot
> hold `1 mon -2 days`. `Interval.from` takes a `Duration`, years and months
> becoming months, weeks and days days and the rest microseconds, and a
> `Duration` parameter binds as one; `toDuration` gives one back where the
> signs agree and throws a `RangeError` where they do not. `timetz` MUST be a
> `DecodeError`: no Temporal type holds a time of day at a fixed offset.

> [spec:pgorm:req:napi.value-tags]
> The module's `Value` MUST declare what plain JavaScript cannot:
> `new Value(data, kind)` takes a kind named as pgorm-python names it, a
> `TypeName` for an enum, or a `CreatedRange` or `CreatedMultirange`;
> `Value.null(kind)` is SQL NULL of a kind; `Value.json(data)` is a JSON
> document, so JSON's `null` stays distinct from SQL NULL; and
> `Value.array(kind, items)` is an array that names its element kind when it
> is empty or NULL. Construction does no I/O and MUST convert at once,
> throwing a `ConstructionError` for data the kind cannot hold exactly. A
> `Value` is immutable and can be bound any number of times, and the plain
> value it hands out is independently owned.
>
> A result read with `{ tagged: true }` MUST give each column as a `Value`
> carrying its kind: an integer's width, an enum's schema-qualified type, an
> array's element kind, and SQL NULL apart from JSON's null.
>
> A created range's kind MUST carry the type's name, schema-qualifiable, and
> its subtype, one of the twelve pgorm's `RangeSubtype` admits, which
> converts each bound. Its value is the text form a `DeriveCreatedRange`
> newtype converts into, bound as text for the statement to cast,
> `CAST($1::text AS name)`, as `Expr::as_range` writes it. A column of one
> MUST decode, through its subtype's binary codec, to a `Range` whose kind is
> named as the column's type is. Arrays of a created range are refused both
> ways.

> [spec:pgorm:req:napi.inference]
> A plain JavaScript parameter MUST be bound as the one kind its type leaves:
> `null` as SQL NULL of no declared kind; a boolean as `bool`; a number as
> `i64` when it is a safe integer other than negative zero, so that it binds
> to an integer column exactly, and as `f64` otherwise; a `bigint` as `i64`; a
> string as `text`; a `Uint8Array` as `bytes`; a `Decimal`, `Uuid`,
> `Interval`, `Temporal.Duration` or Temporal date, time, date-time or
> instant as its kind; a plain object as `json`; and an array as an array of
> the one kind its non-null items infer as, numbers being `i64` items only
> when every one is a safe integer. `undefined`, an empty or all-null array,
> a range, an array of mixed kinds or nested arrays, and any other object
> MUST be refused with a `ConstructionError` that names the explicit form,
> never guessed at or turned into a string.

> [spec:pgorm:req:napi.rows]
> A result's rows MUST be decoded on the runtime thread and reach JavaScript
> as plain objects, one per row, keyed by column name in column order, each
> key an own data property, so that a column named `__proto__` is a property
> rather than the row's prototype. Two columns of one name MUST be a
> `DecodeError`, not one key for both. A statement that returns no rows
> resolves with an empty array.

## Connections

A `Pool` holds the connections, a `Connection` is one of them checked out, a
`Transaction` is a transaction or savepoint on one, and a `RowStream` pulls a
statement's rows over one. Each is a JavaScript object over a boxed native
handle, and each is released by being closed.

> [spec:pgorm:req:napi.connections]
> A `Pool` MUST be built from a connection string and options — TLS mode, CA,
> size, connect and acquire timeouts, statement-cache size and recycling — by
> a constructor that sends nothing and throws a `ConstructionError` for an
> unusable string or option; `connect` resolves with a pool whose server
> answered. TLS MUST default to `verify-full`, verifying the server's
> certificate and host name against the CA given or the platform's trust
> store, with no silent fallback to plaintext; only `sslmode=disable` or
> `tls: "disable"` connects in plaintext. A pool lends a `Connection` with
> `acquire`, and runs a statement of its own on a connection lent to it alone.
>
> Each handle — pool, connection, transaction, stream — MUST be closable
> explicitly, by `close()` or `await using`, and what it holds MUST be
> released by that close, never left to garbage collection or Neon's drop
> queue: closing a connection returns it to its pool at once, and closing a
> pool refuses new work, cancels what runs and resolves once every connection
> is released. A handle collected unclosed releases only what is idle — an
> idle transaction rolled back, an idle connection returned to its pool —
> never cutting short work still running on it, and a transaction keeps the
> connection or transaction it borrows reachable. A connection runs one
> operation at a time: a second, or any while a transaction or stream holds
> it, MUST be refused with a `LifecycleError` rather than queued or raced, as
> is any use after close. Acquiring past `acquireTimeout` MUST reject with a
> `TimeoutError`. Connections live on the runtime and hold nothing a
> JavaScript event loop waits on, so a process that ends holding open
> handles still exits.

> [spec:pgorm:req:napi.results]
> A pool, a connection and a transaction MUST run bound SQL through pgorm's
> cached `ConnectionTrait` path with the same terminals: `execute` resolves
> with the affected-row count as an exact number; `query` with every row;
> `one` with exactly one row; and `optional` with at most one, or `null`. Any
> other count is a `DecodeError`, decided after the query and never in place
> of a database or decode failure. `{ tagged: true }` gives each column as a
> `Value`, as `napi.value-tags` describes.

> [spec:pgorm:req:napi.transactions]
> Transactions MUST run through pgorm's `DatabaseTransaction` and its
> savepoints, a task on the runtime owning each borrowed scope and JavaScript
> sending it owned commands. `begin` opens one explicitly, its `commit` or
> `rollback` usable once; `transaction(fn)` on a pool, a connection or a
> transaction commits when `fn` resolves and rolls back when it throws,
> rejecting with `fn`'s own error whatever the rollback does, and a commit
> that fails ends the transaction and discards its connection. On a
> transaction, `begin` and `transaction` open a savepoint. pgorm's rule that a
> transaction borrows its parent exclusively MUST hold: while a transaction is
> open its connection refuses other work; while a savepoint is open its
> transaction refuses statements, commits, rollbacks and other savepoints;
> and a statement started while another runs on the same transaction is
> refused — each with a `LifecycleError`, never queued or raced. A callback
> that settles with a savepoint still open fails, and commits nothing.
> `mode` and `isolation` select pgorm's `TransactionMode`, the combinations
> PostgreSQL acts on; a savepoint takes neither. Nothing is retried.

> [spec:pgorm:req:napi.cancellation]
> Acquiring a connection, running a statement, opening a transaction or a
> savepoint, and pulling a stream's rows MUST each take an `AbortSignal`. An
> aborted operation rejects with the signal's reason. Its outcome is unknown — a write may or may not
> have happened — so the connection it ran on MUST be discarded rather than
> returned to its pool, and a transaction it ran in ends with nothing
> committed; an operation whose signal fired before it started sends nothing.
> Closing a pool or a connection MUST likewise interrupt what runs on it,
> idle transactions and streams included, without waiting for them.

> [spec:pgorm:req:napi.streams]
> `stream` on a pool or a connection MUST return an async iterator over a
> bound statement's rows through pgorm's `query_raw`, pulling one row per
> `next()`, so that the driver's bounded buffer and the socket hold the server
> back while nothing pulls, and refusing a second pull while one is pending.
> The stream holds its connection until its last row, which frees it, or
> until `return()` — a `for await` loop leaving early — or `close()`, which
> discard it if rows remained; a pool's stream returns or discards a
> connection of its own. A connection's stream reserves that connection, which
> refuses other work meanwhile. A transaction offers no stream, as
> pgorm-python's does not.

## Statements and expressions

JavaScript builds pgorm-query's statements and expressions as pgorm-python
does, in JavaScript's conventions: builder objects whose methods return new
builders, closed choices spelled as strings, and the same terminals that run
SQL text running a built statement.

> [spec:pgorm:req:napi.statements]
> JavaScript MUST build pgorm-query's statements and expressions through
> builder objects that each own the pgorm-query builder state they stand for,
> held by the addon. A builder MUST be immutable: each method returns a new
> builder made from a copy of its receiver's state by the pgorm-query method
> of the same meaning, so a builder reused, or extended in two directions,
> leaves itself and each extension unchanged. A builder is made only by the
> module's functions and the constructors it names; an argument a method
> cannot use MUST be refused as the method is called — a `ConstructionError`,
> or a `TypeError` for an argument of the wrong JavaScript shape — so a
> statement that cannot be built never exists to be run.
>
> `inspect()` MUST give the SQL and the bound values, in placeholder order and
> each a `Value` carrying its kind, that running the builder sends: a
> statement as it runs, an expression as the one item of a `SELECT` and a
> condition as the `WHERE` of `SELECT TRUE`, as pgorm-python inspects them.
> `execute`, `query`, `one`, `optional` and `stream`, on a pool, a connection
> and a transaction, MUST take a built statement where they take SQL text,
> its options second, and build it with the function `inspect()` uses,
> binding its values as any parameter is bound; a parameter list passed
> beside it is a `TypeError`, and an expression or a condition is no statement
> to run. A statement binding more than 65,535 values MUST be refused with a
> `ConstructionError` before anything is sent.

> [spec:pgorm:req:napi.expressions]
> An expression MUST lower into pgorm-query's own builders. `col` is a
> column, qualified by a table and its schema; an operand that is not an
> expression is a value bound as a parameter, never interpolated — inferred as
> a parameter is, or declared with `Value`, an enum's label written cast to its
> type and a created range's text cast to its range through
> `Expr::as_range` — and `bind` makes one an expression. A value pgorm's
> statement values cannot hold MUST be a `ConstructionError` naming the
> explicit form: `null`, which has no kind, and an interval, which pgorm's
> `Value` has no variant for. Every identifier — of a column, table, schema,
> alias, collation, common table expression or type — MUST be a string of
> 1–63 UTF-8 bytes without NUL, minted with `Name::runtime`, so that it is
> quoted wherever pgorm-query writes it, a dot in it part of the name.
>
> Comparison and arithmetic, `||`, `IS [NOT] DISTINCT FROM`, AND, OR, NOT,
> `IS [NOT] NULL`, `[NOT] BETWEEN [SYMMETRIC]` and `[NOT] IN` a list — an
> empty one included, which no value is in — or a subquery MUST each be the
> pgorm-query method of that meaning, and `Condition.all` and `Condition.any`
> pgorm-query's `Condition`, empty ones true and false. `like`, `ilike` and
> their negations take a pattern string, bound, and an optional one-character
> escape; `startsWith`, `endsWith` and `containsText` read their argument as
> text through PostgreSQL's `starts_with`, `right` and `strpos`, so `%` and
> `_` match themselves. `call` reaches only the functions pgorm-query has a
> constructor for, at the argument counts each takes — `uuidv7` with or
> without its shift among them — and another name or count is a
> `ConstructionError`: the name selects a constructor and never reaches SQL.
> A cast names its type by identifier or `TypeName`; `collate` names a
> collation, schema-qualifiable; `at` and `slice` subscript an array; a
> searched or simple `CASE` exists only once it has an arm; `exists` and
> `scalar` take a `Select`.

> [spec:pgorm:req:napi.select]
> `select` MUST build pgorm-query's `SelectStatement`: a projection of
> expressions and aliased expressions, `*` until one is given; FROM items —
> a `Table`, schema-qualified and aliased, or a `FromItem`, which is always
> aliased, a subquery among them —; inner, left, right and full joins on a
> predicate, a lateral subquery among them, and cross joins; WHERE and HAVING,
> each call ANDed to what is there; GROUP BY; ORDER BY with a direction and
> NULLS FIRST or LAST; LIMIT and OFFSET, a non-negative integer number or
> `bigint` within PostgreSQL's `bigint`, or `null` to remove one, anything
> else a `ConstructionError`; DISTINCT; UNION, INTERSECT and EXCEPT, each with
> its ALL form; a row lock of one of the four strengths, OF the items named,
> each by the name it answers to in the statement, and with NOWAIT or SKIP
> LOCKED; and a WITH clause of common table expressions with column lists and
> materialization, or `WITH RECURSIVE` of exactly one, with SEARCH and CYCLE.

> [spec:pgorm:req:napi.writes]
> `insert`, `update` and `deleteFrom` MUST build pgorm-query's INSERT, UPDATE
> and DELETE. An INSERT names its distinct columns before any row; each
> `values` call adds one row of exactly as many operands, by pgorm-query's
> arity check; `select` takes its rows from a `Select` of as many columns
> instead; and `defaultValues` writes one row of defaults, mixed with
> neither. `onConflict` takes only a completed action: `Conflict.doNothing()`,
> answering any conflict, or an arbiter — `Conflict.on` a non-empty list of
> columns and index expressions, with an optional partial-index predicate, or
> `Conflict.onConstraint` a constraint by name, which takes no predicate —
> followed by `doNothing()` or a non-empty update of columns taken from
> EXCLUDED or set to expressions, with an optional `where`. An UPDATE has at
> least one assignment, each column once. An INSERT with no row, an UPDATE
> with no assignment and an UPDATE or DELETE with neither a `where` nor an
> explicit `allRows()` MUST be refused with a `ConstructionError` when it is
> inspected or run. `from` and `using` add the items an UPDATE or a DELETE
> reads. `returning` gives the RETURNING list, every column when it is empty,
> and reads a written row's old and new versions through `ReturningRow.old`
> and `ReturningRow.new`, renamed with `oldAs` and `newAs`. Each statement
> takes a WITH clause, and one with a RETURNING list is a common table
> expression's body.

> [spec:pgorm:req:napi.merge]
> `merge(target, source, on)` MUST follow pgorm-query's MERGE typestate. It
> gives a `PendingMerge`, which has no `inspect()` and which every terminal
> refuses with a `ConstructionError`, because PostgreSQL refuses a MERGE with
> no WHEN arm; its first arm gives the `Merge` statement. `whenMatched` and
> `whenNotMatchedBySource` take an action on a target row —
> `MergeAction.update`, which takes its first assignment and so is never
> empty, `MergeAction.delete()` or `MergeAction.doNothing()` — and
> `whenNotMatched` one on a source row — `MergeAction.insert`, likewise never
> empty, `MergeAction.insertDefaults()` or `MergeAction.doNothing()`; any
> other pairing MUST be a `ConstructionError`. Each arm takes an optional
> condition; within a kind of row the unconditional arm renders after the
> conditional ones and a later one replaces it, as pgorm-query orders them.
> `returning` reads a row's versions as a write's does, `returningAction()`
> puts `merge_action()` first in the list, its only form, and `only()` writes
> `ONLY` before the target. A `Merge` takes a plain WITH clause — a recursive
> one is refused, as PostgreSQL refuses it — and with a RETURNING list is a
> common table expression's body.

> [spec:pgorm:req:napi.sql-json]
> SQL/JSON's query functions `jsonExists`, `jsonValue` and `jsonQuery`, its
> constructors `jsonObject`, `jsonArray`, `jsonArrayQuery`, `jsonObjectAgg`,
> `jsonArrayAgg`, `jsonParse` (`JSON(..)`), `jsonScalar` and `jsonSerialize`,
> `formatJson`, and `isJson` and `isNotJson` MUST lower into pgorm-query's
> SQL/JSON builders, each option applied through the builder's own method. A
> path is a string pgorm-query binds as text cast to `jsonpath`; PASSING takes
> an object of variable names, each an identifier, to values. Behaviours are
> typed per function, and a choice PostgreSQL refuses for a function is a
> `ConstructionError`: `jsonExists` takes `"true"`, `"false"`, `"unknown"` or
> `"error"`; `jsonValue` `"null"`, `"error"` or a `jsonDefault(value)`;
> `jsonQuery` those and `"emptyArray"` or `"emptyObject"`, and one `shaping`
> of `"withWrapper"`, `"withConditionalWrapper"` or `"omitQuotes"`. A DEFAULT
> value is written as an escaped literal, because PostgreSQL refuses a
> parameter there, and one that carries an enum's or a created range's cast is
> refused. RETURNING takes a `DataType` or a built-in type's name, and
> `jsonValue` refuses `json` and `jsonb`. `jsonArrayAgg`'s ordering takes no
> NULLS placement, and one that asks for it is refused rather than dropped.
>
> `jsonTable(context, path, columns, { alias, .. })` MUST build a FROM item
> through `Func::json_table`: its columns a non-empty list of
> `JsonTableColumn.ordinality`, `.value`, `.query`, `.exists` and `.nested`,
> each taking only the clauses its kind of column admits, its alias required,
> its paths written as escaped literals and its PASSING values bound. Its
> `onError` is `"error"` or `"empty"`.

> [spec:pgorm:req:napi.windows]
> `over` MUST attach a window only to what pgorm-query's `WindowFunction`
> admits — a function call, or `jsonArrayAgg` or `jsonObjectAgg` — and
> anything else is a `ConstructionError`; the call under it is a projection
> item, named with `as`. Its window is a `Window`, written inline, or the name
> of the one the SELECT declares with `window(name, window)`. A `Window`
> builds pgorm-query's `WindowStatement` with PARTITION BY, ORDER BY and a
> frame, and a frame MUST follow pgorm-query's frame typestate: a start from
> `FrameType.rows`, `.range` or `.groups` offers only the ends that may follow
> it, so no frame whose end comes before its start can be built; a following
> start does not stand alone as a frame; and `exclude` belongs to a frame.
> `windowFunction` builds PostgreSQL's general-purpose window functions at the
> argument counts each takes, usable only through `over`, which the server
> requires of them; another name or count is a `ConstructionError`.

> [spec:pgorm:req:napi.ranges]
> `contains` (`@>`), `containedBy` (`<@`) and `overlaps` (`&&`) MUST compare
> ranges, multiranges and arrays through pgorm-query's operators of that
> meaning. A range or multirange operand is bound as the `Value` that declares
> its kind — a built-in range or multirange kind, or a `CreatedRange` or
> `CreatedMultirange`, whose text is cast to its type through
> `Expr::as_range` — and a `Range` or `Multirange` passed without one MUST be a
> `ConstructionError`, a range's kind never being inferred.

## Models

A model is JavaScript data: a table and its columns' declarations, made by
`model(name, { schema, columns })` and lowered into the statement builders,
as pgorm-python's `Model` is Python data over its native builders. Its
reads and writes decode into records of its fields, and its relations,
graphs, cursors and pages follow pgorm's own `RelationDef`, `SelectGraph`,
`Cursor` and `Paginator`.

> [spec:pgorm:req:napi.models]
> `model(name, { schema, columns })` MUST declare a model of a table,
> schema-qualified when `schema` is given, from an object of fields, each a
> `column(kind, options)`: a kind every result decodes as — any value kind
> but `u64` and `char`, which no column decodes as — or a schema-qualified
> `TypeName`, `CreatedRange` or `CreatedMultirange`, as a decoded row names
> each type; and the options `name` (the SQL column, the field's own name by
> default), `nullable`, `array`, `primaryKey`, `default`, `generated`
> (`"always"` or `"byDefault"`) and an enum's `values`. Anything else — an
> unknown option, a key column that is nullable, a generated column with a
> default, labels on a column that is no enum, two fields of one column, an
> enum or created range without its schema, an identifier pgorm would not
> take — MUST be a `ConstructionError` thrown as the model is declared.
> Declaring a model MUST send nothing, run no DDL and claim no Rust entity:
> no derive, `EntityTrait` or ActiveModel hook is behind it. A model and its
> columns never change; `as(alias)` gives another model reading the table
> under an alias. Its columns are qualified as pgorm qualifies an entity's,
> by the table's alias or else its name, never its schema. The TypeScript
> declarations MUST type a model from its declaration alone: `RowOf`, a
> record's fields and their types, a nullable column's `| null` and an array
> column's items, an enum's listed values as a union of their labels;
> `InsertOf`, requiring every field neither nullable, defaulted nor
> generated and refusing a field generated always; `UpdateOf`; and `KeyOf`.

> [spec:pgorm:req:napi.model-records]
> A model's terminals MUST decode each row into a record: a plain object
> keyed by field, in declaration order, its values those `napi.values`
> declares. Each field MUST be found by the result column its statement
> projected it under, and the result's columns MUST be exactly those; each
> column's kind MUST be the kind its field declares, compared as the addon
> spells a decoded column's kind — a scalar's name, an enum's
> schema-qualified type, a created range's type and subtype, an array's
> element — so an `int8` read into an `i32` field, or another schema's enum,
> is a `DecodeError` rather than a value of the wrong type. A NULL in a field
> not nullable, and a label an enum field does not list, MUST be a
> `DecodeError`, never a value. A model's rows stream from a pool or a
> connection as records, through the same decode.

> [spec:pgorm:req:napi.model-reads]
> `find()` MUST select every field, `select(..)` the fields it names, and
> `findByKey(key)` the row of a primary key, through pgorm-query's
> `SelectStatement`; `where` ANDs a condition, `orderBy`, `limit` and
> `offset` are the builder's, and `join(relation)` joins a relation's far
> end, from the query's model or a table it joined, without reading its
> columns. `col(field)` MUST be the field's column as an expression whose
> comparisons convert a value through the field's declared kind — an enum's
> label bound cast to its type, as pgorm's column comparisons cast through
> `save_as` — and take an expression as written, a `Value` only of the
> field's own kind, and `null` never, which `isNull()` tests. `key(values)`
> MUST match every primary-key field and no other. `all`, `one`,
> `optional` and `count` run on a pool, a connection or a transaction —
> `one` exactly one row and `optional` at most one, any other count a
> `DecodeError` — and `count` counts as `napi.pagination` does.

> [spec:pgorm:req:napi.model-writes]
> `insert(values)`, `insertMany(rows)`, `update(values)` and `delete()` MUST
> build pgorm-query's INSERT, UPDATE and DELETE of a model's table. A field
> left out of `values` MUST stay out of the statement — an insert takes the
> table's default, an update leaves the column as it is — and a field set to
> `null` MUST be written as SQL NULL, which only a nullable field takes; each
> value converts through its field's declared kind as a comparison's does,
> and the fields set are written in declaration order whatever order the
> object names them in. An unknown field, an `undefined` value, a field
> generated always, an insert leaving out a field neither nullable,
> defaulted nor generated, an update setting nothing and rows of one insert
> setting different fields MUST be `ConstructionError`s, and an insert into
> an aliased model is refused. An insert of no rows sends nothing:
> `execute` resolves with 0 and the returning terminals with no rows. An
> UPDATE or DELETE MUST have a `where` or `allRows()` before it runs, as
> `napi.writes` requires. `returning(..)` reads the fields written as
> records. `returningChange` and `returningChanges` on an update, and
> `returningUpsert` and `returningUpserts` on an insert, MUST read each
> written row's two versions as pgorm's `exec_returning_change(s)` and
> `exec_returning_upsert(s)` do: every field of both versions in a
> `RETURNING WITH (OLD AS pgorm_old, NEW AS pgorm_new)` list, the old under
> `o_` and the new under `n_`, a change being `{ old, new }` and an upsert
> `{ kind: "inserted", new }` where the old version is NULL in every column
> and `{ kind: "updated", old, new }` otherwise; a row an insert's conflict
> clause did not write is `null` from `returningUpsert` and left out of
> `returningUpserts`.

> [spec:pgorm:req:napi.relations]
> A model's `belongsTo`, `hasOne` and `hasMany` MUST make a relation to
> another model pairing fields of each, one or a non-empty list of equal
> length, in order. A join along it MUST be the condition pgorm's
> `join_condition` writes for a relation's columns, each pair equal and the
> pairs ANDed, between the tables as the join names them. `find(row)` MUST
> read the far end's rows whose paired fields equal the row's, and none
> when one of the row's is NULL. `load(db, rows)` MUST read the far end of
> every row in one query of the distinct keys: a list of records per row for
> `hasMany`; otherwise a record per row, or `null` where the row's key holds
> a NULL; a key nothing at the far end matches, or that a `hasOne` relation
> matches twice, MUST be a `DecodeError`, never a missing row.

> [spec:pgorm:req:napi.graphs]
> `graph()` MUST read a model's rows with the rows its relations reach as
> pgorm's `SelectGraph` does, its SQL the SQL `SelectGraph` writes for the
> same tables, relations and aliases. The slot kind MUST be the join type
> and the decode shape: `joinOne` an INNER JOIN decoded as a record,
> `joinMaybe` a LEFT JOIN decoded as a record or `null`, and `via` a LEFT
> JOIN of a table no record reads. A relation is joined from the first
> table of its model the graph reads, or the decoded source `from` names,
> and a slot or hop answers to its table's name or the `alias` it is given;
> a second table answering to a name already read MUST be a
> `ConstructionError`. Every decoded source is projected under its own
> prefix, `s0_` for the root and `s{i}_` for the i-th slot, each name
> composed as pgorm's `result_column_name` composes it — under 63 bytes as
> it stands, otherwise bounded with its FNV-1a hash. A slotless graph
> decodes as the root's record and a graph with slots as a tuple of the
> root's and each slot's, an optional slot `null` exactly where every
> column it reads is NULL, as pgorm's absence witness reads it; a present
> slot that does not decode MUST be a `DecodeError`, never an absent one.
> `col(source, field)` qualifies a field as a decoded source is named.
> `allGrouped` reads a graph of one slot as each root with its slot's
> records, ordering by the root's primary key behind the graph's ordering
> and grouping by the decoded root's key, as pgorm's `all_grouped` does.

> [spec:pgorm:req:napi.cursors]
> `cursor(..fields)` on a model query or a graph MUST page by keyset as
> pgorm's `Cursor` does: ordered by its fields, then — on a graph — by the
> root's primary key and each slot's, in declaration order, as tiebreaks, a
> root key field already ordered by not repeated; that ordering replacing the
> query's. `after` and `before` take a boundary of the order fields' values,
> `afterWith` and `beforeWith` of the order fields' or the whole key's,
> each converted through its field's declared kind and any other arity a
> `ConstructionError`. A boundary of n values MUST be the row-value
> comparison written out as n disjuncts, the k-th holding the first k-1
> columns equal and comparing the k-th — greater past an ascending
> cursor's `after`, less short of its `before`, the reverse when it
> descends. `first(n)` and `last(n)` limit the window, a `last` window
> read in the reversed order and returned in the cursor's own, and each
> replaces the other; `asc()` and `desc()` choose the direction.

> [spec:pgorm:req:napi.pagination]
> `paginate(pageSize)` on a model query or a graph MUST read page `n` as
> the query with a limit of `pageSize` and an offset of `pageSize × n`,
> replacing its own, as pgorm's `Paginator` does, a page size of at least 1
> and an offset past `Number.MAX_SAFE_INTEGER` refused. `numItems` and a
> query's `count` MUST count the query's rows with its limit, offset and
> ordering dropped, `SELECT COUNT(*) AS "num_items" FROM (..) AS
> "sub_query"`, and resolve with an exact number; `numPages` rounds the
> pages up; `pages` yields each page from the first until one is empty.

## Clean exit

> [spec:pgorm:req:napi.exit]
> Loading the addon MUST NOT keep a process alive, nor MUST anything it leaves
> behind once its operations have settled: a process that has finished its
> work exits on its own, in Node.js and in Deno, with its ordinary status and
> nothing written to stderr. An operation still in flight when a script's last
> statement runs MUST keep the process up until it settles. A process that
> exits explicitly while operations are running MUST neither wait for them nor
> crash, and an outcome that arrives after its instance has been torn down — the
> process exiting, its worker terminated — MUST be dropped without a panic or a
> write to a closed environment. The shared channel and the drop queue MUST
> NOT hold a process open once nothing is in flight, whether or not the drop
> queue has deliveries pending. An unhandled rejection MUST end the
> process the way the runtime ends one for any other unhandled rejection.

## Types and tests

> [spec:pgorm:req:napi.typing]
> The module MUST ship TypeScript declarations for every export, error classes
> and their fields included, beside it as `lib/index.d.ts` and named by the
> module's `@ts-self-types` directive so Deno finds them as Node.js tooling
> does. `deno check` MUST type-check the declarations, the module and the test
> suite, which uses the public API through them.

> [spec:pgorm:req:napi.runtimes+1]
> The binding targets runtimes that carry `Temporal` as a global, which its
> date and time values are: Node.js 26 or later, and Deno 2.9.5 or later, the
> oldest release tested. Loading the module in a runtime without `Temporal`
> MUST throw an error naming the requirement rather than fail later on a
> value. Node.js 26 strips TypeScript types without a flag, so the suite and
> its fixtures run under plain `node`.
>
> One test suite MUST run unchanged under `node --test` and under `deno test`,
> written against `node:test` and `node:assert`, which both runners provide,
> with every runtime difference — how a fixture process is started, and the
> Deno permissions it is granted — kept in one support module. The exit
> behaviour `napi.exit` requires MUST be tested in fresh processes of
> the runtime under test, each held to exiting by itself before a deadline. CI
> MUST run the suite in both runtimes, Node.js 26 and Deno, against PostgreSQL
> 18 on Linux and macOS.
