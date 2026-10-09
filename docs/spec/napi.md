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

## Schema

JavaScript builds DDL through pgorm-query's DDL builders as pgorm-python's
schema surface does, and further: every statement `sql-ddl.md` specifies that
a schema is made of, in the builder conventions the statements above use.

> [spec:pgorm:req:napi.schema]
> Tables, their columns, keys and constraints and their alterations, indexes,
> types, sequences, extensions and comments MUST be built through
> pgorm-query's DDL builders, each JavaScript object owning the builder state
> it stands for, immutable as a statement builder is: a method returns a new
> builder from a copy, and an argument it cannot use is refused as it is
> called — a `ConstructionError`, or a `TypeError` for an argument of the
> wrong JavaScript shape, an options object holding a key the builder does
> not know among them, so that a misspelt option is never left out of a
> statement. Constructing one does no I/O, and nothing runs DDL but a
> terminal.
>
> PostgreSQL takes no parameters in DDL, so a DDL statement MUST render
> through pgorm-query's own rendering, which writes every value it carries —
> a `DEFAULT`, a `CHECK`'s operands, an enum label, a comment, an extension's
> version — as an escaped literal, and MUST run through `execute` and the
> other terminals as that SQL text with no values beside it; `inspect()` gives
> the same SQL and an empty value list. No name or value is concatenated into
> SQL by the binding: every identifier is minted with `Name::runtime` and
> quoted where pgorm-query writes it. A table or sequence is named by a
> string or by a `Table`, whose schema qualifies it and which MUST NOT carry an
> alias; a type by a string or a `TypeName`.
>
> pgorm-query's typestates hold: `alterTable`, `alterType` and
> `alterSequence` name their object and nothing more, since PostgreSQL parses
> no `ALTER` without an action, so each MUST have no `inspect()`, every
> terminal MUST refuse it with a `ConstructionError`, and its first action
> gives the statement. A `ColumnDef` is no statement, and a terminal refuses
> it likewise.

> [spec:pgorm:req:napi.schema-tables]
> `createTable(table)` MUST build pgorm-query's `TableCreateStatement`:
> columns, each a `ColumnDef` with a type, `ifNotExists`, the table's one
> primary key — a later `primaryKey` replacing it — any number of unique
> keys, foreign keys and `CHECK` constraints. A key is a column or a
> non-empty list of them, with a name, `INCLUDE`d columns, a deferrability
> and PostgreSQL 18's `withoutOverlaps` column, which pgorm-query writes last
> as `"c" WITHOUT OVERLAPS`; `nullsNotDistinct` is the unique key's alone. A
> foreign key pairs its columns in order with as many referenced columns,
> anything else refused, and takes a name, `onDelete` and `onUpdate`
> actions, a deferrability, an enforcement and PostgreSQL 18's `PERIOD` pair,
> written last on both sides. A `CHECK` is an expression with a name, an
> enforcement and `noInherit`. Deferrability is `"notDeferrable"`,
> `"deferrableInitiallyImmediate"` or `"deferrableInitiallyDeferred"`, and
> enforcement `"enforced"` or `"notEnforced"`; neither is written unless it
> is given.
>
> `new ColumnDef(name, type)` takes a `DataType`, a built-in type's name, a
> `TypeName` or a created range, and the clauses pgorm-query writes in the
> order they are added: `notNull`, the column's one `NOT NULL` constraint,
> with a name and `noInherit`; `null`; `default`, an expression or a value;
> `check`; `generated(expression, "stored" | "virtual")`, whose kind MUST be
> named, PostgreSQL 17 refusing a generated column without one and 18
> reading it as virtual; `identity("always" | "byDefault", options)`, its
> sequence's options those `napi.schema-sequences` gives; `autoIncrement`,
> the serial family; and `collate`, the column's one collation. A column
> with no type is refused where a table is created or a column added.
> `dropTable` (one or more tables, `ifExists`, a behavior), `renameTable`,
> whose new name is bare, `renameColumn`, `renameConstraint` and
> `truncateTable` MUST build the statements pgorm-query has for each.

> [spec:pgorm:req:napi.schema-alter]
> The first action on `alterTable(table)` MUST give pgorm-query's
> `TableAlterStatement`, which takes more, each the pgorm-query action of
> its name: `addColumn` (with `ifNotExists`), `modifyColumn`, `dropColumn`,
> `addPrimaryKey`, `addUnique`, `addForeignKey`, `addCheck`, `addNotNull`
> (PostgreSQL 18's table-level `NOT NULL`, with a name and `noInherit`),
> `dropConstraint` of any kind by name (with `ifExists` and a behavior),
> `validateConstraint`, `alterConstraint` (`"inherit"`, `"noInherit"`,
> `"enforced"` or `"notEnforced"`), `setExpression` and `dropExpression`
> (with `ifExists`). `notValid` belongs to the three actions PostgreSQL takes
> it on — `addForeignKey`, `addCheck` and `addNotNull` — and leaves the rows
> already there unchecked until `validateConstraint` checks them, while new
> rows are held to the constraint at once.
>
> `modifyColumn` writes each aspect its column carries as pgorm-query writes
> it — a retype with its collation, `SET` or `DROP NOT NULL`, a named `NOT
> NULL` added, `SET DEFAULT`, a `CHECK` added, an identity added. An aspect
> no such action writes MUST be refused rather than dropped: a generated
> expression (which `setExpression` and `dropExpression` change), the serial
> family, and a collation without the type it is given with; so MUST a
> column that changes nothing.

> [spec:pgorm:req:napi.schema-indexes]
> `createIndex(table, entry, { name })` MUST build pgorm-query's
> `IndexCreateStatement` over its first entry, and `column` appends more. An
> entry is a column's name, an expression, or `{ on, order, operatorClass }`
> over either, `order` being `"asc"` or `"desc"`. The index takes `unique`,
> `nullsNotDistinct` — which PostgreSQL defines for a unique index alone, and
> so makes the index unique rather than be written for nothing — `ifNotExists`,
> `using(method)`, an access method by identifier, `include` and `where`, a
> partial index's predicate ANDed to one already there. `CONCURRENTLY` is not
> offered, PostgreSQL refusing it in a transaction. `dropIndex(table, name)`
> drops the index from its table's schema, with `ifExists`.

> [spec:pgorm:req:napi.schema-types]
> `createType(name)` MUST build pgorm-query's `TypeCreateStatement`, a shell
> type until a kind is chosen, its kind one slot as pgorm-query holds it:
> `asEnum` and `values(labels)`, which appends; `asComposite` and
> `attribute(name, type, { collation })`, which appends; or `asRange(subtype,
> { subtypeOpclass, collation, subtypeDiff, multirangeTypeName })`. An enum
> label is data, written as a literal, and MUST be at most 63 bytes without
> NUL, as PostgreSQL stores it; the empty label is one.
>
> The change on `alterType(name)` MUST give its statement: `addValue(label, {
> before } | { after })`, never both; `renameTo`, whose new name is bare;
> `renameValue`; `renameAttribute`, a statement of its own with a behavior; or
> a composite's `addAttribute`, `dropAttribute` (with `ifExists`) and
> `alterAttribute`, which give a statement that takes more of them and a
> behavior, `"cascade"` carrying the changes into typed tables.
> `dropType(names, { ifExists, behavior })` drops one or more types.
> A collation is a name, or `{ name, schema }`.

> [spec:pgorm:req:napi.schema-sequences]
> A sequence's options MUST be one vocabulary for a standalone sequence and
> an identity column, pgorm-query's `SequenceOptions`: `{ incrementBy,
> minValue, maxValue, startWith, cache, cycle }`, each number a safe-integer
> number or a `bigint` within `bigint`, a bound's `null` its `NO` form, and a
> key outside them a `TypeError`. `createSequence(name)` takes
> `ifNotExists`, `asType` (`"smallint"`, `"integer"` or `"bigint"`),
> `options`, which merges at least one option into those set, and `ownedBy(table,
> column)`, `ownedBy(null)` being `OWNED BY NONE`; the first clause on
> `alterSequence(name)` gives its statement, which takes the same clauses,
> `restart(value?)` and `ifExists`. `dropSequence` and `renameSequence` build
> pgorm-query's statements. `createExtension(name, { ifNotExists, schema,
> version, cascade })` and `dropExtension(name, { ifExists, behavior })` build
> pgorm-query's extension statements, the version a literal; and
> `commentOnTable` and `commentOnColumn` its `COMMENT ON`, the text a literal
> pgorm-query escapes.

## Pipelines

JavaScript composes pgorm's PRQL-shaped pipeline — `pgorm::pipeline` — as
pgorm-python does: relation-to-relation stages over sources, compiled through
prqlc to PostgreSQL SQL, with runtime values entering through a binder whose
placeholders belong to the stage that minted them.

> [spec:pgorm:req:napi.pipeline]
> The module's `pipeline` namespace MUST compose `pgorm::pipeline::Pipeline`,
> each JavaScript object owning the pgorm state it stands for and each stage
> returning a new pipeline from a copy: `from(source)` and the stages
> `filter`, `derive`, `select`, `group(..)` followed by `aggregate(..)`,
> `window(over, ..)`, `sort`, `take(n)`, `takeRange(start, end)`,
> `join(source, on, { kind })`, `append`, `intersect`, `remove` and
> `distinct`. A source is a `Table`, schema-qualified and read under its
> alias when it has one, a table's name, another `Pipeline` embedded whole
> with its bound values, or `source(relation).named(name)`, which reads a
> relation under a name of its own. A grouping is no pipeline until it is
> aggregated, so `group` gives a `Grouped` whose only way back is
> `aggregate`, and which no terminal runs. A row count is an integer, never
> an expression, as PRQL refuses a bound `LIMIT`. A registered Rust entity
> as a source, and the `select_sources` terminal that decodes one, wait on
> entities registering with the binding and are not offered.
>
> A `Pipeline` is a statement for `execute`, `query`, `one`, `optional` and
> `stream` on a pool, a connection and a transaction, compiled through
> `Pipeline::into_sql` as `inspect()` compiles it, its values bound as any
> statement's are, `one` and `optional` keeping their cardinality. What
> pgorm's compile step judges — a name it cannot write as one, a reserved
> alias, a relation or column prqlc cannot resolve — MUST be a
> `ConstructionError` from `inspect()` or the terminal, before anything is
> sent.

> [spec:pgorm:req:napi.pipeline-expressions]
> Pipeline expressions MUST be their own objects, lowered into
> `pgorm::pipeline::Expr` only as a stage takes them, never mixed with the
> statement builders' `Expr`: `col(table, column)`, qualified as prqlc
> requires; `alias(name)`, a name a stage introduces, read back unqualified;
> `thisColumn` and `thatColumn`, a join's two sides; the operators `eq` ..
> `lte`, `and`, `or`, `not`, `neg`, `add` .. `rem`, `coalesce`, `isNull`,
> `isNotNull`, `inArray`, `cast` over pgorm's closed `CastType` set, `as`,
> `asc` and `desc`; `caseWhen(arms, otherwise)`; and the aggregates and
> window functions pgorm's pipeline has, each at the argument count it takes.
> `over()` builds a window's partition, ordering and `rows` or `range`
> frame.
>
> Values reach a pipeline's SQL by one of two routes, and the spelling says
> which. `literal(value)` — `null`, a boolean, a safe-integer number or a
> `bigint` within `bigint`, a finite number, a string — is written into the
> SQL as pgorm's pipeline writes a literal. Any other value an operand is
> given MUST be bound: inferred or declared with `Value` as a parameter is,
> and minted as a placeholder by the binder of the stage that takes it, never
> written as a literal; a value with no kind (`null`), an interval, and a
> value whose kind needs a cast pgorm's pipeline cannot write (an enum's or a
> created range's) are refused with a `ConstructionError` naming the explicit
> form. A window's partition and ordering take no value, bound or not, as
> pgorm's `Over` takes none.

> [spec:pgorm:req:napi.pipeline-binder]
> Each expression-taking stage MUST have a `With` form — `filterWith`,
> `deriveWith`, `selectWith`, `groupWith`, `aggregateWith`, `windowWith`,
> `sortWith` and `joinWith` — that calls its function once, synchronously,
> with a `Binder`, whose `bind(value)` mints one placeholder for one value,
> reusable within the stage. A placeholder is branded with the scope of the
> call that minted it, as pgorm's binder brands it with a lifetime: once its
> function has returned or thrown, the binder MUST refuse to bind, and an
> expression carrying a placeholder MUST be refused, each with a
> `LifecycleError`, as any handle used after it closed is; so MUST one that
> combines two scopes' placeholders, and one given to a stage other than the
> one its function returns it to — a plain stage, another call's or another
> pipeline's — or to a window's partition or ordering. The function MUST
> return synchronously, a promise being a `TypeError`: an operand for a
> filter or a join condition, and an expression or an array of operands for
> a list-taking stage, anything else there a `TypeError`. An array of more
> than 32 is a `ConstructionError`: pgorm's list-taking `_with` stages take a
> fixed-size array, which the binding dispatches up to that bound, where a
> stage that binds nothing takes a list of any length.

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
